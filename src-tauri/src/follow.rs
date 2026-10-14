//! 翻译窗口跟着游戏窗口走：游戏切到前台时带到游戏上面，游戏最小化时一起最小化、还原时一起还原。
//!
//! 用系统的 WinEvent，不轮询；只调整 Z 序、不激活自己，键盘焦点留在游戏里。
//! 没有用“把游戏设为本窗口的 owner”：跨进程的 owner 会把两边的输入队列绑在一起，游戏一卡我们也跟着卡。

#[cfg(windows)]
pub fn start(app: tauri::AppHandle) {
    imp::start(app)
}

#[cfg(not(windows))]
pub fn start(_app: tauri::AppHandle) {}

#[cfg(windows)]
mod imp {
    use crate::session::Session;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;
    use tauri::{AppHandle, Manager};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, GetWindowThreadProcessId, IsIconic, SetWindowPos, ShowWindow,
        EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, HWND_TOP, MSG,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE,
        WINEVENT_OUTOFCONTEXT,
    };

    static APP: OnceLock<AppHandle> = OnceLock::new();
    /// 翻译窗口是我们跟着游戏最小化的。用户自己最小化的不要擅自还原
    static AUTO_MIN: AtomicBool = AtomicBool::new(false);

    pub fn start(app: AppHandle) {
        if APP.set(app).is_err() {
            return;
        }
        // 事件回调派发到注册钩子的线程，这个线程得一直跑消息循环
        std::thread::spawn(|| unsafe {
            let hook = |from, to| SetWinEventHook(from, to, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT);
            if hook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND).is_invalid()
                || hook(EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND).is_invalid()
            {
                println!("[rust] follow: SetWinEventHook 失败");
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
        });
    }

    unsafe extern "system" fn on_event(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _thread: u32,
        _time: u32,
    ) {
        let Some(app) = APP.get() else { return };
        let Some(game) = app.state::<Session>().game_pid() else { return };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != game {
            return;
        }
        let Some(ours) = app.get_webview_window("main").and_then(|w| w.hwnd().ok()) else { return };
        // tauri 依赖的 windows crate 版本可能和我们的不同，按裸指针转一下
        let ours = HWND(ours.0 as _);
        unsafe {
            if event == EVENT_SYSTEM_MINIMIZESTART {
                if !IsIconic(ours).as_bool() {
                    let _ = ShowWindow(ours, SW_SHOWMINNOACTIVE);
                    AUTO_MIN.store(true, Ordering::Relaxed);
                }
                return;
            }
            // 游戏回来了（还原或切到前台）
            if IsIconic(ours).as_bool() {
                if !AUTO_MIN.swap(false, Ordering::Relaxed) {
                    return;
                }
                let _ = ShowWindow(ours, SW_SHOWNOACTIVATE);
            }
            let _ = SetWindowPos(ours, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}
