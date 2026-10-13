//! 玩过的游戏：下次自动连接，并自动锁定上次手动选过的那一路钩子。
//!
//! 以 exe 路径为键存进应用配置目录的 games.json。H-code 形如 `HQX-C@20DA90:game.exe`，
//! 地址相对模块基址，游戏重启后依然有效，所以能跨会话复用。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    /// 手动锁定过的钩子；None 表示交给自动选择
    #[serde(default)]
    pub hcode: Option<String>,
    /// 检测到游戏启动时自动连接
    #[serde(default = "yes")]
    pub auto_attach: bool,
    pub last_played: u64,
}

fn yes() -> bool {
    true
}

pub struct Games {
    file: Option<PathBuf>,
    list: Mutex<Vec<Game>>,
}

/// 游戏的键：exe 路径不分大小写、去掉 `\\?\` 前缀。同一个 exe 从不同 API 拿到的写法可能不同。
/// 历史记录也按它分游戏存。
pub fn key(path: &str) -> String {
    path.trim_start_matches(r"\\?\").to_lowercase()
}

fn same(a: &str, b: &str) -> bool {
    key(a) == key(b)
}

impl Games {
    pub fn load(file: Option<PathBuf>) -> Self {
        let list = file
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { file, list: Mutex::new(list) }
    }

    fn save(&self, list: &[Game]) {
        let Some(file) = &self.file else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(list) {
            if let Err(e) = std::fs::write(file, json) {
                println!("[rust] 保存游戏记录失败: {e}");
            }
        }
    }

    fn update(&self, path: &str, f: impl FnOnce(&mut Game)) {
        let mut list = self.list.lock().unwrap();
        if let Some(g) = list.iter_mut().find(|g| same(&g.path, path)) {
            f(g);
            self.save(&list);
        }
    }

    /// 最近玩过的在前
    pub fn all(&self) -> Vec<Game> {
        let mut v = self.list.lock().unwrap().clone();
        v.sort_by(|a, b| b.last_played.cmp(&a.last_played));
        v
    }

    pub fn find(&self, path: &str) -> Option<Game> {
        self.list.lock().unwrap().iter().find(|g| same(&g.path, path)).cloned()
    }

    /// 记一次游玩：新游戏就加进来，老游戏刷新名字和时间
    pub fn touch(&self, path: &str, name: &str, title: Option<String>, now: u64) {
        let mut list = self.list.lock().unwrap();
        match list.iter_mut().find(|g| same(&g.path, path)) {
            Some(g) => {
                g.name = name.to_string();
                if title.is_some() {
                    g.title = title;
                }
                g.last_played = now;
            }
            None => list.push(Game {
                path: path.to_string(),
                name: name.to_string(),
                title,
                hcode: None,
                auto_attach: true,
                last_played: now,
            }),
        }
        self.save(&list);
    }

    pub fn set_hcode(&self, path: &str, hcode: Option<String>) {
        self.update(path, |g| g.hcode = hcode);
    }

    pub fn set_auto(&self, path: &str, on: bool) {
        self.update(path, |g| g.auto_attach = on);
    }

    pub fn forget(&self, path: &str) {
        let mut list = self.list.lock().unwrap();
        list.retain(|g| !same(&g.path, path));
        self.save(&list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_across_reload() {
        let file = std::env::temp_dir().join(format!("fukidashi-games-{}.json", std::process::id()));
        let g = Games::load(Some(file.clone()));
        g.touch(r"E:\GAME\a.exe", "a.exe", Some("A".into()), 1);
        g.set_hcode(r"\\?\e:\game\A.EXE", Some("HQX-C@20DA90:a.exe".into()));
        g.touch(r"E:\GAME\b.exe", "b.exe", None, 2);

        let g = Games::load(Some(file.clone()));
        let a = g.find(r"e:\game\a.exe").unwrap();
        assert_eq!(a.hcode.as_deref(), Some("HQX-C@20DA90:a.exe"));
        assert!(a.auto_attach);
        assert_eq!(g.all()[0].name, "b.exe", "最近玩过的在前");

        g.forget(r"E:\GAME\a.exe");
        assert!(g.find(r"E:\GAME\a.exe").is_none());
        let _ = std::fs::remove_file(file);
    }
}
