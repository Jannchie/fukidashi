mod furigana;
mod backlog;
mod games;
mod history;
mod migrate;
mod model;
mod normalize;
mod pipeline;
mod proc;
mod source;
mod translate;

use model::{Event, HookLine, ProcessInfo, Sentence, ThreadStat};
use pipeline::Pipeline;
use source::{lunahook::LunaHookSource, replay::ReplaySource, Source};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use games::{Game, Games};
use tauri_plugin_window_state::StateFlags;
use history::History;
use translate::{Translator, TranslatorConfig};

pub(crate) const EVENT: &str = "fukidashi://event";
/// 线程统计的推送节流。正文事件不节流 —— 那是用户在等的东西。
const STATS_INTERVAL: Duration = Duration::from_millis(400);
/// 连上游戏时先显示最近这么多句，再往前要用户往上翻
const BACKLOG: usize = 200;

struct Inner {
    pipeline: Pipeline,
    stop: Option<Arc<AtomicBool>>,
    /// 不入库的句子（演示模式）的 id；入库的句子用库里的行号
    next_id: u64,
    /// 当前连着的游戏（pid, exe 路径），用来把锁定的钩子和读过的句子记到对应的游戏上
    game: Option<(u32, String)>,
    /// 用户手动断开的进程：自动连接不要马上又连回去
    suppressed: Option<u32>,
}

pub struct AppState {
    inner: Mutex<Inner>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                pipeline: Pipeline::new(),
                stop: None,
                next_id: history::EPHEMERAL_BASE,
                game: None,
                suppressed: None,
            }),
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn emit(app: &AppHandle, ev: Event) {
    let _ = app.emit(EVENT, ev);
}

/// 停掉当前正在跑的源（如果有）。返回是否真的停了一个。
fn stop_current(state: &AppState) -> bool {
    let mut inner = state.inner.lock().unwrap();
    let running = inner.stop.take().inspect(|flag| flag.store(true, Ordering::Relaxed));
    inner.pipeline.reset();
    inner.game = None;
    running.is_some()
}

/// 启动一个源，并把它接到处理循环上。
fn start(
    app: AppHandle,
    state: State<'_, AppState>,
    src: Box<dyn Source>,
    game: Option<(u32, String)>,
    detail: String,
) -> Result<(), String> {
    stop_current(&state);
    app.state::<Translator>().reset();

    // 先把这个游戏之前读过的推给前端，新句子接在后面
    let seen = game.as_ref().map(|(_, path)| {
        let history = app.state::<History>();
        emit(&app, Event::Backlog { game: path.clone(), page: history.page(path, None, BACKLOG) });
        history.recent_texts(path, 3000)
    });

    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut inner = state.inner.lock().unwrap();
        inner.pipeline.seed(seen.unwrap_or_default());
        inner.stop = Some(stop.clone());
        inner.game = game;
    }

    let (tx, rx) = channel::<HookLine>();

    // 生产者：读源
    {
        let stop = stop.clone();
        let app = app.clone();
        std::thread::spawn(move || {
            let result = src.run(tx, stop.clone());
            // 源自己结束了（比如游戏退出）：标记为空闲，自动连接才能接手下一次启动
            if let Some(state) = app.try_state::<AppState>() {
                let mut inner = state.inner.lock().unwrap();
                if inner.stop.as_ref().is_some_and(|s| Arc::ptr_eq(s, &stop)) {
                    inner.stop = None;
                    inner.game = None;
                }
            }
            if let Err(e) = result {
                emit(&app, Event::Status { connected: false, detail: format!("来源已断开：{e}") });
            }
        });
    }

    // 消费者：喂流水线、推事件
    {
        let app2 = app.clone();
        std::thread::spawn(move || {
            let mut last_stats = Instant::now() - STATS_INTERVAL;
            while let Ok(line) = rx.recv() {
                let Some(state) = app2.try_state::<AppState>() else { break };

                let (emitted, game) = {
                    let mut inner = state.inner.lock().unwrap();
                    // 必须用流水线归一化后的文本，绝不能回头去取 line.text
                    let text = inner.pipeline.feed(&line);
                    (text, inner.game.as_ref().map(|(_, path)| path.clone()))
                };

                if let Some(text) = emitted {
                    let at = now_ms();
                    let stored = game.and_then(|g| app2.state::<History>().add(&g, &line.key, &text, at));
                    let id = stored.unwrap_or_else(|| {
                        let mut inner = state.inner.lock().unwrap();
                        inner.next_id += 1;
                        inner.next_id
                    });
                    let translator = app2.state::<Translator>();
                    if translator.enabled() {
                        translator.submit(id, text.clone());
                    }
                    let ruby = furigana::annotate(&text);
                    emit(
                        &app2,
                        Event::Sentence(Sentence { id, key: line.key.clone(), text, ruby, at }),
                    );
                }

                if last_stats.elapsed() >= STATS_INTERVAL {
                    last_stats = Instant::now();
                    let (threads, active, warming_up) = {
                        let inner = state.inner.lock().unwrap();
                        (
                            inner.pipeline.stats(),
                            inner.pipeline.active().cloned(),
                            inner.pipeline.warming_up(),
                        )
                    };
                    emit(&app2, Event::Threads { threads, active, warming_up });
                }
            }
        });
    }

    emit(&app, Event::Status { connected: true, detail });
    Ok(())
}

/// 把前端日志转到进程标准输出。
///
/// 桌面应用没有随手可开的控制台，前端出问题时从外面看只是"界面没反应"。
/// 有了这条通道，dev 模式下前端发生了什么可以直接在终端里读到。
#[tauri::command]
fn log_front(level: String, msg: String) {
    println!("[front:{level}] {msg}");
}

#[tauri::command]
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

/// 附加到游戏。手动选择和自动连接都走这里。
fn attach(app: &AppHandle, pid: u32, name: &str, path: &str, title: Option<String>, auto: bool) -> Result<(), String> {
    // 注入的钩子必须和游戏同位数
    let is64 = proc::exe_is_64bit(path)
        .ok_or_else(|| format!("读不出 {name} 的 PE 头，判断不了是 32 位还是 64 位"))?;
    let (dir, version) = source::lunahook::locate(app).map_err(|e| e.to_string())?;
    let bits = if is64 { 64 } else { 32 };
    println!("[rust] attach pid={pid} name={name} {bits} 位 LunaHook {version}{}", if auto { "（自动）" } else { "" });

    // 要连的就是正连着的那个：沿用当前会话，不断开重连
    {
        let state = app.state::<AppState>();
        let inner = state.inner.lock().unwrap();
        if inner.stop.is_some() && inner.game.as_ref().is_some_and(|(p, _)| *p == pid) {
            return Ok(());
        }
    }

    let games = app.state::<Games>();
    let label = title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| name.to_string());
    let remembered = games.find(path).and_then(|g| g.hcode);
    games.touch(path, name, title, now_ms());

    let state = app.state::<AppState>();
    let src = LunaHookSource { dir, pid, is64 };
    let verb = if auto { "已自动连接" } else { "已附加到" };
    let game = Some((pid, path.to_string()));
    start(app.clone(), state.clone(), Box::new(src), game, format!("{verb} {label} · {bits} 位"))?;

    let mut inner = state.inner.lock().unwrap();
    inner.suppressed = None;
    inner.pipeline.prefer(remembered);
    Ok(())
}

#[tauri::command]
fn attach_process(app: AppHandle, pid: u32, name: String, path: String, title: Option<String>) -> Result<(), String> {
    attach(&app, pid, &name, &path, title, false)
}

/// 往上翻：取某个游戏 `before` 之前的一页
#[tauri::command]
fn history_page(history: State<'_, History>, game: String, before: Option<u64>, limit: usize) -> history::Page {
    history.page(&game, before, limit.min(1000))
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

/// 后台盯着有窗口的进程：玩过且开着自动连接的游戏一启动就连上。
///
/// 窗口要连续两轮都在才动手 —— 刚弹出窗口时引擎可能还没加载完，钩子会认不出引擎。
fn watch_games(app: AppHandle) {
    std::thread::spawn(move || {
        let mut candidate: Option<u32> = None;
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let (busy, suppressed) = {
                let state = app.state::<AppState>();
                let inner = state.inner.lock().unwrap();
                (inner.stop.is_some(), inner.suppressed)
            };
            let games = app.state::<Games>();
            if busy || !games.all().iter().any(|g| g.auto_attach) {
                candidate = None;
                continue;
            }
            let found = proc::windowed().into_iter().find(|(pid, _, path, _)| {
                Some(*pid) != suppressed && games.find(path).is_some_and(|g| g.auto_attach)
            });
            match found {
                Some((pid, name, path, title)) if candidate == Some(pid) => {
                    candidate = None;
                    if let Err(e) = attach(&app, pid, &name, &path, Some(title), true) {
                        emit(&app, Event::Status { connected: false, detail: format!("自动连接失败：{e}") });
                    }
                }
                Some((pid, ..)) => candidate = Some(pid),
                None => candidate = None,
            }
        }
    });
}

#[tauri::command]
fn set_translator(translator: State<'_, Translator>, config: TranslatorConfig) {
    translator.configure(config);
}

#[tauri::command]
async fn list_models(base_url: String, api_key: String) -> Result<Vec<String>, String> {
    translate::list_models(&base_url, &api_key).await
}

/// 无游戏的演示 / 压测模式。
#[tauri::command]
fn start_replay(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
    interval_ms: Option<u64>,
) -> Result<(), String> {
    let src = ReplaySource::new(path.as_deref(), interval_ms.unwrap_or(1200));
    let n = src.lines.len();
    start(app, state, Box::new(src), None, format!("演示模式 · {n} 句循环"))
}

#[tauri::command]
fn stop(app: AppHandle, state: State<'_, AppState>) {
    let pid = state.inner.lock().unwrap().game.as_ref().map(|(pid, _)| *pid);
    stop_current(&state);
    state.inner.lock().unwrap().suppressed = pid;
    emit(&app, Event::Status { connected: false, detail: "已停止".into() });
}

/// 手动锁定某一路来源；传 None 交回自动选择。
#[tauri::command]
fn pin_thread(app: AppHandle, state: State<'_, AppState>, games: State<'_, Games>, key: Option<String>) {
    let (threads, active, warming_up) = {
        let mut inner = state.inner.lock().unwrap();
        inner.pipeline.pin(key);
        // 记到游戏上，下次连接自动锁定同一路
        if let Some((_, path)) = &inner.game {
            games.set_hcode(path, inner.pipeline.pinned_hcode());
        }
        (inner.pipeline.stats(), inner.pipeline.active().cloned(), inner.pipeline.warming_up())
    };
    emit(&app, Event::Threads { threads, active, warming_up });
}

#[tauri::command]
fn get_threads(state: State<'_, AppState>) -> (Vec<ThreadStat>, Option<String>) {
    let inner = state.inner.lock().unwrap();
    (inner.pipeline.stats(), inner.pipeline.active().cloned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    migrate::from_old_name(&context.config().identifier);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState::default())
        // 记位置大小，但不记边框：标题栏是自己画的，旧版存下的“有边框”不能还原回来
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::DECORATIONS)
                .build(),
        )
        .setup(|app| {
            app.manage(Translator::new(app.handle().clone()));
            let file = app.path().app_config_dir().ok().map(|d| d.join("games.json"));
            app.manage(Games::load(file));
            let db = app.path().app_data_dir().ok().map(|d| d.join("history.db"));
            app.manage(History::open(db.as_deref()));
            furigana::warm_up();
            watch_games(app.handle().clone());
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
            get_threads,
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
                if stop_current(&app.state::<AppState>()) {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                }
            }
        });
}
