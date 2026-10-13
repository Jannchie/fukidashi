//! LunaHook —— LunaTranslator 的钩子核心，文件由 `pnpm lunahook:update` 从其发布包中抽取。
//!
//! 分工：`LunaHost64.dll` 加载进 Fukidashi 进程，负责与游戏通信、按调用点把文本分路；
//! `LunaHook{32,64}.dll` 由官方注入器 `LunaSubprocess{32,64}.exe` 注入到游戏里。
//! 我们只依赖 `Luna_*` 这层 C 接口，LunaTranslator 自己也是这么调的，跟着它升级成本最低。

use super::Source;
use crate::model::HookLine;
use anyhow::{bail, Context, Result};
use std::ffi::{c_char, c_void, CStr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// 不给注入器弹控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 注入后等游戏连回来的上限。被杀毒软件拦截时注入器会"成功"返回但永远连不上。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct LunaHookSource {
    /// 存放 LunaHook 文件的目录
    pub dir: PathBuf,
    pub pid: u32,
    /// 游戏是否 64 位，决定注入哪一份钩子
    pub is64: bool,
}

/// 一路文本的身份。按值传递，布局必须与 LunaHost 的 ThreadParam 一致。
#[repr(C)]
#[derive(Clone, Copy)]
struct ThreadParam {
    pid: u32,
    addr: u64,
    ctx: u64,
    ctx2: u64,
}

type W = *const u16;
type ProcessEvent = extern "C" fn(u32);
type ThreadCreate = extern "C" fn(W, *const c_char, ThreadParam, bool);
type ThreadDestroy = extern "C" fn(W, *const c_char, ThreadParam);
type Output = extern "C" fn(W, *const c_char, ThreadParam, W);
type HostInfo = extern "C" fn(i32, W);
type HookInsert = extern "C" fn(u32, u64, W);
type Embed = extern "C" fn(W, ThreadParam);
type I18nQuery = extern "C" fn(W) -> *mut c_void;
type EmuGameInfo = extern "C" fn(W, W, W);

struct Api {
    connect: extern "C" fn(u32),
    need_inject: extern "C" fn(u32) -> bool,
    detach: extern "C" fn(u32),
}

/// 当前接收文本的那一路 channel。LunaHost 的回调是裸函数指针，只能经由全局状态转交。
static SINK: Mutex<Option<Sender<HookLine>>> = Mutex::new(None);
/// 已连上的游戏 pid，0 表示没有
static CONNECTED: AtomicU32 = AtomicU32::new(0);
/// 同一时刻只跑一个源。重新附加时旧源的收尾（detach、清空 SINK）必须先于新源开始，
/// 否则会把新源刚装上的 SINK 清掉。
static RUNNING: Mutex<()> = Mutex::new(());

/// LunaHost 是进程级单例：`Luna_Start` 只能调一次，所以首次使用时加载并启动，之后一直复用。
fn api(dir: &Path) -> Result<&'static Api> {
    static API: OnceLock<std::result::Result<Api, String>> = OnceLock::new();
    API.get_or_init(|| unsafe { load(dir) }.map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("加载 LunaHost64.dll 失败：{e}"))
}

#[cfg(windows)]
unsafe fn load(dir: &Path) -> Result<Api> {
    use windows::core::{s, HSTRING, PCSTR};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH};

    // 按 dll 自己的目录找依赖：VC++ 运行库随它放在同一目录，用户机器上没装运行库也能加载
    let path = HSTRING::from(dir.join("LunaHost64.dll").as_os_str());
    let lib = LoadLibraryExW(&path, None, LOAD_WITH_ALTERED_SEARCH_PATH)?;
    let sym = |name: PCSTR| {
        GetProcAddress(lib, name).with_context(|| format!("缺少导出函数 {}", name.display()))
    };

    let start: extern "C" fn(
        ProcessEvent, ProcessEvent, ThreadCreate, ThreadDestroy, Output,
        HostInfo, HookInsert, Embed, I18nQuery, EmuGameInfo,
    ) = std::mem::transmute(sym(s!("Luna_Start"))?);
    let settings: extern "C" fn(i32, i32, i32, i32, bool) =
        std::mem::transmute(sym(s!("Luna_Settings"))?);
    let api = Api {
        connect: std::mem::transmute(sym(s!("Luna_ConnectProcess"))?),
        need_inject: std::mem::transmute(sym(s!("Luna_CheckIfNeedInject"))?),
        detach: std::mem::transmute(sym(s!("Luna_DetachProcess"))?),
    };

    start(
        on_connect, on_disconnect, on_thread_create, on_thread_destroy, on_output,
        on_host_info, on_hook_insert, on_embed, on_i18n_query, on_emu_game_info,
    );
    // 与 LunaTranslator 的默认值一致：500ms 内没有新字就算一句结束，默认按 Shift-JIS 解码，
    // 不自动装通用 GDI 钩子 —— 那些钩子几乎只产生噪声，引擎钩子才是正文来源。
    settings(500, 932, 3000, 1_000_000, false);
    Ok(api)
}

#[cfg(not(windows))]
unsafe fn load(_: &Path) -> Result<Api> {
    bail!("LunaHook 只支持 Windows")
}

fn wide(p: W) -> String {
    if p.is_null() {
        return String::new();
    }
    let len = (0..).take_while(|&i| unsafe { *p.add(i) } != 0).count();
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

extern "C" fn on_connect(pid: u32) {
    CONNECTED.store(pid, Ordering::Relaxed);
}

extern "C" fn on_disconnect(pid: u32) {
    let _ = CONNECTED.compare_exchange(pid, 0, Ordering::Relaxed, Ordering::Relaxed);
}

extern "C" fn on_output(hcode: W, name: *const c_char, tp: ThreadParam, text: W) {
    let line = HookLine {
        key: format!("{}:{:x}:{:x}:{:x}", tp.pid, tp.addr, tp.ctx, tp.ctx2),
        name: if name.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()
        },
        hcode: wide(hcode),
        text: wide(text),
    };
    if let Some(tx) = SINK.lock().unwrap().as_ref() {
        let _ = tx.send(line);
    }
}

extern "C" fn on_host_info(_kind: i32, msg: W) {
    println!("[lunahook] {}", wide(msg));
}

extern "C" fn on_hook_insert(_pid: u32, _addr: u64, hcode: W) {
    println!("[lunahook] 已装钩子 {}", wide(hcode));
}

extern "C" fn on_thread_create(_: W, _: *const c_char, _: ThreadParam, _: bool) {}
extern "C" fn on_thread_destroy(_: W, _: *const c_char, _: ThreadParam) {}
extern "C" fn on_embed(_: W, _: ThreadParam) {}
/// 返回空指针表示不翻译，LunaHost 会用原文
extern "C" fn on_i18n_query(_: W) -> *mut c_void {
    std::ptr::null_mut()
}
extern "C" fn on_emu_game_info(_: W, _: W, _: W) {}

impl LunaHookSource {
    fn inject(&self) -> Result<()> {
        let arch = if self.is64 { "64" } else { "32" };
        let exe = self.dir.join(format!("LunaSubprocess{arch}.exe"));
        let dll = self.dir.join(format!("LunaHook{arch}.dll"));
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("dllinject").arg(self.pid.to_string()).arg(&dll);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd.status().with_context(|| format!("启动注入器失败：{}", exe.display()))?;
        println!("[lunahook] 注入 pid={} {arch} 位，注入器返回 {status}", self.pid);
        Ok(())
    }
}

impl Source for LunaHookSource {
    fn run(self: Box<Self>, tx: Sender<HookLine>, stop: Arc<AtomicBool>) -> Result<()> {
        let api = api(&self.dir)?;
        let _running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        *SINK.lock().unwrap() = Some(tx);

        let result = (|| {
            (api.connect)(self.pid);
            // 上次没卸载干净时钩子还留在游戏里，这里会直接重连而不必再注入
            if (api.need_inject)(self.pid) {
                self.inject()?;
            }

            let deadline = Instant::now() + CONNECT_TIMEOUT;
            while CONNECTED.load(Ordering::Relaxed) != self.pid {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                if Instant::now() > deadline {
                    bail!("注入后游戏没有连回来：可能被杀毒软件拦截，或游戏以管理员身份运行（此时 Fukidashi 也需要管理员身份）");
                }
                std::thread::sleep(Duration::from_millis(50));
            }

            while !stop.load(Ordering::Relaxed) {
                if CONNECTED.load(Ordering::Relaxed) != self.pid {
                    bail!("游戏已退出");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok(())
        })();

        // 无论怎么结束都要卸载：钩子留在游戏里会一直往一个没人读的管道里写。
        // 卸载是异步的：等它真正断开再放行下一个源，否则紧接着的重新附加会撞上
        // 一个正在卸载的钩子（报"已经注入"，随即断开），白白落空。
        (api.detach)(self.pid);
        let deadline = Instant::now() + Duration::from_secs(3);
        while CONNECTED.load(Ordering::Relaxed) == self.pid && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        *SINK.lock().unwrap() = None;
        result
    }
}

/// 解析 LunaHook 文件目录，并返回其版本号。
///
/// 打包后在 resource 目录里；`cargo run` 开发时资源还没被复制过去，回退到源码树中的 resources/。
pub fn locate(app: &tauri::AppHandle) -> Result<(PathBuf, String)> {
    use tauri::path::BaseDirectory;
    use tauri::Manager;

    const REL: &str = "resources/lunahook";
    let candidates = [
        app.path().resolve(REL, BaseDirectory::Resource).ok(),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REL)),
    ];
    for dir in candidates.into_iter().flatten() {
        // Tauri 解析出的路径带 `\\?\` 前缀，注入器把它拼进游戏进程里 LoadLibrary 时处理不了
        let dir = PathBuf::from(dir.to_string_lossy().trim_start_matches(r"\\?\"));
        if dir.join("LunaHost64.dll").is_file() {
            let version = std::fs::read_to_string(dir.join("VERSION")).unwrap_or_default();
            return Ok((dir, version.trim().to_string()));
        }
    }
    bail!("找不到 LunaHook 文件，请先运行 pnpm lunahook:update")
}
