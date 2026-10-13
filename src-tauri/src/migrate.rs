//! 改名迁移：应用 ID 从 com.jannchie.yomu 改成了 com.jannchie.fukidashi，
//! 配置目录跟着变了。第一次以新名字启动时把旧目录整个拷过来：
//! Roaming 里是 games.json 和窗口位置，Local 里是 WebView 的 localStorage（阅读、翻译偏好）。
//! 必须在创建窗口之前跑，WebView 一启动就会锁住自己的数据目录。

use std::path::Path;

const OLD: &str = "com.jannchie.yomu";

pub fn from_old_name(identifier: &str) {
    for var in ["APPDATA", "LOCALAPPDATA"] {
        let Some(base) = std::env::var_os(var) else { continue };
        let base = Path::new(&base);
        let (old, new) = (base.join(OLD), base.join(identifier));
        if old.is_dir() && !new.exists() {
            match copy_dir(&old, &new) {
                Ok(()) => println!("[rust] 已迁移旧配置 {} -> {}", old.display(), new.display()),
                Err(e) => println!("[rust] 迁移旧配置失败 {}: {e}", old.display()),
            }
        }
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            // 锁文件之类拷不动的跳过，不影响数据本身
            let _ = std::fs::copy(entry.path(), dest);
        }
    }
    Ok(())
}
