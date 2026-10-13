//! 一次连接的生命周期：启动文本源、把每一行送进流水线、产出的句子入库、翻译、推给前端。
//!
//! 手动附加、自动连接、演示模式都从 [`start`] 进来；同一时间只跑一个源。

use crate::furigana;
use crate::games::Games;
use crate::history::{self, History, Page};
use crate::model::{Event, HookLine, Sentence};
use crate::pipeline::Pipeline;
use crate::proc;
use crate::source::{self, lunahook::LunaHookSource, Source};
use crate::translate::Translator;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

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
    /// 最近一次推送的连接状态 (connected, detail)
    status: (bool, String),
}

pub struct Session {
    inner: Mutex<Inner>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                pipeline: Pipeline::new(),
                stop: None,
                next_id: history::EPHEMERAL_BASE,
                game: None,
                suppressed: None,
                status: (false, "未连接".into()),
            }),
        }
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn emit(app: &AppHandle, ev: Event) {
    let _ = app.emit(crate::EVENT, ev);
}

/// 界面加载完成时取的当前状态。连接和历史都是一次性推送的事件，
/// 自动连接可能发生在界面开始监听之前，只靠事件会漏掉。
#[derive(serde::Serialize)]
pub struct SessionInfo {
    pub connected: bool,
    pub detail: String,
    /// 正连着的游戏（exe 路径）；演示模式和未连接时为 None
    pub game: Option<String>,
}

/// 给一页历史标上振假名。振假名不入库：现算很快，词典更新后旧记录也能跟着变好
pub fn with_ruby(mut page: Page) -> Page {
    for s in &mut page.sentences {
        s.ruby = furigana::annotate(&s.text);
    }
    page
}

impl Session {
    /// 更新连接状态并推给前端
    pub fn set_status(&self, app: &AppHandle, connected: bool, detail: String) {
        self.inner.lock().unwrap().status = (connected, detail.clone());
        emit(app, Event::Status { connected, detail });
    }

    /// 当前状态；顺带推一次来源统计，界面上的来源栏也一起补上
    pub fn info(&self, app: &AppHandle) -> SessionInfo {
        let inner = self.inner.lock().unwrap();
        emit(app, inner.pipeline.snapshot());
        SessionInfo {
            connected: inner.status.0,
            detail: inner.status.1.clone(),
            game: inner.game.as_ref().map(|(_, path)| path.clone()),
        }
    }

    /// 停掉当前正在跑的源（如果有）。返回是否真的停了一个。
    pub fn stop_current(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let running = inner.stop.take().inspect(|flag| flag.store(true, Ordering::Relaxed));
        inner.pipeline.reset();
        inner.game = None;
        running.is_some()
    }

    /// 用户手动断开：停掉，并且自动连接先别连回这个进程
    pub fn stop_by_user(&self) {
        let pid = self.inner.lock().unwrap().game.as_ref().map(|(pid, _)| *pid);
        self.stop_current();
        self.inner.lock().unwrap().suppressed = pid;
    }

    /// 手动锁定某一路来源（None 交回自动选择），并记到游戏上，下次连接自动锁定同一路
    pub fn pin(&self, games: &Games, key: Option<String>) -> Event {
        let mut inner = self.inner.lock().unwrap();
        inner.pipeline.pin(key);
        if let Some((_, path)) = &inner.game {
            games.set_hcode(path, inner.pipeline.pinned_hcode());
        }
        inner.pipeline.snapshot()
    }
}

/// 启动一个源，并把它接到处理循环上。
pub fn start(app: &AppHandle, src: Box<dyn Source>, game: Option<(u32, String)>, detail: String) {
    let session = app.state::<Session>();
    session.stop_current();
    app.state::<Translator>().reset();

    // 先把这个游戏之前读过的推给前端，新句子接在后面
    let seen = game.as_ref().map(|(_, path)| {
        let history = app.state::<History>();
        let page = with_ruby(history.page(path, None, BACKLOG));
        emit(app, Event::Backlog { game: path.clone(), page });
        history.recent_texts(path, crate::backlog::MEMORY)
    });

    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut inner = session.inner.lock().unwrap();
        inner.pipeline.seed(seen.unwrap_or_default());
        inner.stop = Some(stop.clone());
        inner.game = game;
    }

    let (tx, rx) = channel::<HookLine>();

    // 生产者：读源
    {
        let app = app.clone();
        std::thread::spawn(move || {
            let result = src.run(tx, stop.clone());
            // 源自己结束了（比如游戏退出）：标记为空闲，自动连接才能接手下一次启动
            {
                let session = app.state::<Session>();
                let mut inner = session.inner.lock().unwrap();
                if inner.stop.as_ref().is_some_and(|s| Arc::ptr_eq(s, &stop)) {
                    inner.stop = None;
                    inner.game = None;
                }
            }
            if let Err(e) = result {
                app.state::<Session>().set_status(&app, false, format!("来源已断开：{e}"));
            }
        });
    }

    // 消费者：喂流水线、推事件
    {
        let app = app.clone();
        std::thread::spawn(move || {
            let mut last_stats = Instant::now() - STATS_INTERVAL;
            while let Ok(line) = rx.recv() {
                consume(&app, &line);
                if last_stats.elapsed() >= STATS_INTERVAL {
                    last_stats = Instant::now();
                    let snapshot = app.state::<Session>().inner.lock().unwrap().pipeline.snapshot();
                    emit(&app, snapshot);
                }
            }
        });
    }

    session.set_status(app, true, detail);
}

/// 一行钩子文本：成句的话入库、送翻译、推给前端
fn consume(app: &AppHandle, line: &HookLine) {
    let session = app.state::<Session>();
    let (text, game) = {
        let mut inner = session.inner.lock().unwrap();
        // 必须用流水线归一化后的文本，绝不能回头去取 line.text
        let Some(text) = inner.pipeline.feed(line) else { return };
        (text, inner.game.as_ref().map(|(_, path)| path.clone()))
    };

    let at = now_ms();
    let stored = game.and_then(|g| app.state::<History>().add(&g, &line.key, &text, at));
    let id = stored.unwrap_or_else(|| {
        let mut inner = session.inner.lock().unwrap();
        inner.next_id += 1;
        inner.next_id
    });
    let translating = app.state::<Translator>().submit(id, &text);
    let ruby = furigana::annotate(&text);
    emit(app, Event::Sentence(Sentence { id, key: line.key.clone(), text, ruby, at, translating }));
}

/// 附加到游戏。手动选择和自动连接都走这里。
pub fn attach(app: &AppHandle, pid: u32, name: &str, path: &str, title: Option<String>, auto: bool) -> Result<(), String> {
    // 注入的钩子必须和游戏同位数
    let is64 = proc::exe_is_64bit(path)
        .ok_or_else(|| format!("读不出 {name} 的 PE 头，判断不了是 32 位还是 64 位"))?;
    let (dir, version) = source::lunahook::locate(app).map_err(|e| e.to_string())?;
    let bits = if is64 { 64 } else { 32 };
    println!("[rust] attach pid={pid} name={name} {bits} 位 LunaHook {version}{}", if auto { "（自动）" } else { "" });

    let session = app.state::<Session>();
    // 要连的就是正连着的那个：沿用当前会话，不断开重连
    {
        let inner = session.inner.lock().unwrap();
        if inner.stop.is_some() && inner.game.as_ref().is_some_and(|(p, _)| *p == pid) {
            return Ok(());
        }
    }

    let games = app.state::<Games>();
    let label = title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| name.to_string());
    let remembered = games.find(path).and_then(|g| g.hcode);
    games.touch(path, name, title, now_ms());

    let src = LunaHookSource { dir, pid, is64 };
    let verb = if auto { "已自动连接" } else { "已附加到" };
    start(app, Box::new(src), Some((pid, path.to_string())), format!("{verb} {label} · {bits} 位"));

    let mut inner = session.inner.lock().unwrap();
    inner.suppressed = None;
    inner.pipeline.prefer(remembered);
    Ok(())
}

/// 后台盯着有窗口的进程：玩过且开着自动连接的游戏一启动就连上。
///
/// 窗口要连续两轮都在才动手 —— 刚弹出窗口时引擎可能还没加载完，钩子会认不出引擎。
pub fn watch_games(app: AppHandle) {
    std::thread::spawn(move || {
        let mut candidate: Option<u32> = None;
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let (busy, suppressed) = {
                let session = app.state::<Session>();
                let inner = session.inner.lock().unwrap();
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
                        app.state::<Session>().set_status(&app, false, format!("自动连接失败：{e}"));
                    }
                }
                Some((pid, ..)) => candidate = Some(pid),
                None => candidate = None,
            }
        }
    });
}
