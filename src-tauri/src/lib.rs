mod backlog;
mod furigana;
mod games;
mod history;
mod migrate;
mod model;
mod normalize;
mod pipeline;
mod proc;
mod session;
mod source;
mod translate;

use games::{Game, Games};
use history::History;
use model::ProcessInfo;
use session::{emit, Session};
use source::replay::ReplaySource;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_window_state::StateFlags;
use translate::{Translator, TranslatorConfig};

pub(crate) const EVENT: &str = "fukidashi://event";

/// 把前端日志转到进程标准输出。
///
/// 桌面应用没有随手可开的控制台，前端出问题时从外面看只是"界面没反应"。
/// 有了这条通道，dev 模式下前端发生了什么可以直接在终端里读到。
#[tauri::command]
fn log_front(level: String, msg: String) {
    println!("[front:{level}] {msg}");
}

// 下面几条命令要读文件、枚举进程、给一页历史分词，标成 async 放到后台线程，不卡界面

#[tauri::command(async)]
fn list_processes() -> Vec<ProcessInfo> {
    let t = std::time::Instant::now();
    let v = proc::list();
    println!("[rust] list_processes -> {} 个进程, 耗时 {:?}", v.len(), t.elapsed());
    v
}

/// 内置 LunaHook 的版本号；文件缺失时返回错误说明。
#[tauri::command]
fn hook_status(app: AppHandle) -> Result<String, String> {
    source::lunahook::locate(&app).map(|(_, v)| v).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn attach_process(app: AppHandle, pid: u32, name: String, path: String, title: Option<String>) -> Result<(), String> {
    session::attach(&app, pid, &name, &path, title, false)
}

/// 往上翻：取某个游戏 `before` 之前的一页
#[tauri::command(async)]
fn history_page(app: AppHandle, game: String, before: Option<u64>, limit: usize) -> history::Page {
    session::with_ruby(app.state::<History>().page(&game, before, limit.min(1000)))
}

#[tauri::command]
fn list_games(games: State<'_, Games>) -> Vec<Game> {
    games.all()
}

#[tauri::command]
fn set_game_auto(games: State<'_, Games>, path: String, on: bool) {
    games.set_auto(&path, on);
}

#[tauri::command]
fn forget_game(games: State<'_, Games>, path: String) {
    games.forget(&path);
}

#[tauri::command]
fn set_translator(translator: State<'_, Translator>, config: TranslatorConfig) {
    translator.configure(config);
}

#[tauri::command]
async fn list_models(base_url: String, api_key: String) -> Result<Vec<String>, String> {
    translate::list_models(&base_url, &api_key).await
}

/// 无游戏的演示 / 压测模式。`path` 可指向一份剧本 JSON 做压测（见 replay.rs），界面上的演示不传。
#[tauri::command]
fn start_replay(app: AppHandle, path: Option<String>, interval_ms: Option<u64>) {
    let src = ReplaySource::new(path.as_deref(), interval_ms.unwrap_or(1200));
    let n = src.lines.len();
    session::start(&app, Box::new(src), None, format!("演示模式 · {n} 句循环"));
}

#[tauri::command]
fn stop(app: AppHandle, session: State<'_, Session>) {
    session.stop_by_user();
    session.set_status(&app, false, "已停止".into());
}

/// 界面加载完成时取一次当前状态，补上加载前错过的推送
#[tauri::command]
fn session_info(app: AppHandle, session: State<'_, Session>) -> session::SessionInfo {
    session.info(&app)
}

/// 手动锁定某一路来源；传 None 交回自动选择。
#[tauri::command]
fn pin_thread(app: AppHandle, session: State<'_, Session>, games: State<'_, Games>, key: Option<String>) {
    emit(&app, session.pin(&games, key));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    migrate::from_old_name(&context.config().identifier);
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Session::default())
        // 记位置大小，但不记边框：标题栏是自己画的，旧版存下的“有边框”不能还原回来
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::DECORATIONS)
                .build(),
        )
        .setup(|app| {
            let file = app.path().app_config_dir().ok().map(|d| d.join("games.json"));
            app.manage(Games::load(file));
            let db = app.path().app_data_dir().ok().map(|d| d.join("history.db"));
            app.manage(History::open(db.as_deref()));
            let handle = app.handle().clone();
            app.manage(Translator::new(app.handle().clone(), move |id, text| {
                handle.state::<History>().set_translation(id, text)
            }));
            furigana::warm_up();
            session::watch_games(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            log_front,
            hook_status,
            list_processes,
            attach_process,
            start_replay,
            stop,
            pin_thread,
            session_info,
            set_translator,
            list_models,
            list_games,
            history_page,
            set_game_auto,
            forget_game
        ])
        .build(context)
        .expect("error while running tauri application")
        .run(|app, ev| {
            // 退出时给源一点时间收尾：要先把钩子从游戏里卸载，不然它会一直留在游戏进程里。
            if let tauri::RunEvent::Exit = ev {
                if app.state::<Session>().stop_current() {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                }
            }
        });
}
