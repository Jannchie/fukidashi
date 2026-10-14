//! 叠加模式的鼠标穿透。
//!
//! 整个窗口穿透时网页收不到任何鼠标事件，没法再点按钮退出；所以由这里盯着光标：
//! 光标落在前端报上来的几块“把手”区域（工具条、缩放角）里时接收鼠标，其余地方一律穿透给游戏。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};

/// 把手区域，窗口内的 CSS 像素：[x, y, w, h]
pub type Rect = [f64; 4];

static HOT: Mutex<Vec<Rect>> = Mutex::new(Vec::new());
static RUNNING: AtomicBool = AtomicBool::new(false);
const POLL: Duration = Duration::from_millis(30);

pub fn set(app: &AppHandle, on: bool, hot: Vec<Rect>) {
    let Some(win) = app.get_webview_window("main") else { return };
    *HOT.lock().unwrap() = hot;
    // 叠加时不要窗口阴影：透明窗口外面一圈阴影很扎眼
    let _ = win.set_shadow(!on);
    if !on {
        RUNNING.store(false, Ordering::Relaxed);
        let _ = win.set_ignore_cursor_events(false);
        return;
    }
    if RUNNING.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(move || {
        let mut passing: Option<bool> = None;
        while RUNNING.load(Ordering::Relaxed) {
            let pass = !over_handle(&win);
            if passing != Some(pass) {
                let _ = win.set_ignore_cursor_events(pass);
                passing = Some(pass);
            }
            std::thread::sleep(POLL);
        }
    });
}

fn over_handle(win: &WebviewWindow) -> bool {
    let (Ok(cursor), Ok(pos), Ok(scale)) = (win.cursor_position(), win.inner_position(), win.scale_factor()) else {
        return false;
    };
    let x = (cursor.x - pos.x as f64) / scale;
    let y = (cursor.y - pos.y as f64) / scale;
    HOT.lock().unwrap().iter().any(|[l, t, w, h]| x >= *l && x < l + w && y >= *t && y < t + h)
}
