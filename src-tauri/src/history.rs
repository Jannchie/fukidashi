//! 读过的每一句原文和译文都存下来，按游戏分开，重连后接着往上翻。
//!
//! 存在应用数据目录的 history.db（SQLite）。行号直接当句子 id 用：跨会话唯一，
//! 译文晚到时按 id 回填同一行，前端也拿它对上译文。

use crate::games::key as game_key;
use crate::model::Sentence;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

/// 演示模式之类不入库的句子用这一段 id，和库里的行号永远撞不上（仍小于 JS 的安全整数上限）
pub const EPHEMERAL_BASE: u64 = 1 << 50;

pub struct History {
    db: Option<Mutex<Connection>>,
}

/// 一页历史：从旧到新排好，附带已有的译文
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub sentences: Vec<Sentence>,
    pub translations: Vec<(u64, String)>,
    /// 更早的还有没有
    pub more: bool,
}

impl History {
    pub fn open(file: Option<&Path>) -> Self {
        let db = file.and_then(|f| {
            if let Some(dir) = f.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match Connection::open(f).and_then(|c| init(&c).map(|_| c)) {
                Ok(c) => Some(Mutex::new(c)),
                Err(e) => {
                    println!("[rust] 打开历史记录失败，本次不保存: {e}");
                    None
                }
            }
        });
        Self { db }
    }

    #[cfg(test)]
    fn memory() -> Self {
        let c = Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        Self { db: Some(Mutex::new(c)) }
    }

    /// 记一句原文，返回它的 id；库不可用时返回 None
    pub fn add(&self, game: &str, key: &str, text: &str, at: u64) -> Option<u64> {
        let db = self.db.as_ref()?.lock().unwrap();
        db.prepare_cached("INSERT INTO lines (game, source, text, at) VALUES (?1, ?2, ?3, ?4)")
            .and_then(|mut st| st.execute(params![game_key(game), key, text, at as i64]))
            .inspect_err(|e| println!("[rust] 写历史失败: {e}"))
        .ok()?;
        Some(db.last_insert_rowid() as u64)
    }

    /// 回填译文。不在库里的 id（演示模式）自然什么也不改
    pub fn set_translation(&self, id: u64, text: &str) {
        let Some(db) = &self.db else { return };
        let db = db.lock().unwrap();
        let _ = db
            .prepare_cached("UPDATE lines SET translation = ?1 WHERE id = ?2")
            .and_then(|mut st| st.execute(params![text, id as i64]));
    }

    /// 取 `before` 之前（不含）最近的 `limit` 句；`before` 为 None 时取最新的。
    /// 句子不带振假名，由调用方按需标注
    pub fn page(&self, game: &str, before: Option<u64>, limit: usize) -> Page {
        let empty = Page { sentences: vec![], translations: vec![], more: false };
        let Some(db) = &self.db else { return empty };
        let db = db.lock().unwrap();
        let before = before.map_or(i64::MAX, |b| b as i64);
        let rows: Vec<(u64, String, String, Option<String>, u64)> = db
            .prepare_cached(
                "SELECT id, source, text, translation, at FROM lines
                 WHERE game = ?1 AND id < ?2 ORDER BY id DESC LIMIT ?3",
            )
            .and_then(|mut st| {
                st.query_map(params![game_key(game), before, limit as i64 + 1], |r| {
                    Ok((r.get::<_, i64>(0)? as u64, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? as u64))
                })?
                .collect()
            })
            .unwrap_or_default();

        let more = rows.len() > limit;
        let mut page = empty;
        page.more = more;
        for (id, key, text, tr, at) in rows.into_iter().take(limit).rev() {
            if let Some(tr) = tr {
                page.translations.push((id, tr));
            }
            page.sentences.push(Sentence { id, key, text, ruby: None, at, translating: false });
        }
        page
    }

    /// 这个游戏最近读过的 `limit` 句原文，从旧到新
    pub fn recent_texts(&self, game: &str, limit: usize) -> Vec<String> {
        let Some(db) = &self.db else { return vec![] };
        let db = db.lock().unwrap();
        let mut v: Vec<String> = db
            .prepare_cached("SELECT text FROM lines WHERE game = ?1 ORDER BY id DESC LIMIT ?2")
            .and_then(|mut st| st.query_map(params![game_key(game), limit as i64], |r| r.get(0))?.collect())
            .unwrap_or_default();
        v.reverse();
        v
    }

    /// 某个游戏一共记了多少句
    #[cfg(test)]
    fn count(&self, game: &str) -> u64 {
        let db = self.db.as_ref().unwrap().lock().unwrap();
        db.query_row("SELECT COUNT(*) FROM lines WHERE game = ?1", [game_key(game)], |r| r.get::<_, i64>(0))
            .unwrap() as u64
    }
}

fn init(c: &Connection) -> rusqlite::Result<()> {
    // WAL：边写边读不互相阻塞，断电也不容易坏库。
    // WAL 下 NORMAL 只在检查点时落盘：断电最多丢最后几句，换来每句入库不用等一次 fsync
    c.pragma_update(None, "journal_mode", "WAL")?;
    c.pragma_update(None, "synchronous", "NORMAL")?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS lines (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            game        TEXT NOT NULL,
            source      TEXT NOT NULL,
            text        TEXT NOT NULL,
            translation TEXT,
            at          INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS lines_game ON lines (game, id);",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_back_through_one_game() {
        let h = History::memory();
        let ids: Vec<u64> = (0..5).map(|i| h.add(r"E:\G\a.exe", "k", &format!("あ{i}"), i)).map(Option::unwrap).collect();
        h.add(r"E:\G\b.exe", "k", "べつ", 9).unwrap();
        h.set_translation(ids[4], "啊4");

        let latest = h.page(r"\\?\e:\g\A.EXE", None, 2);
        assert_eq!(latest.sentences.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["あ3", "あ4"]);
        assert_eq!(latest.translations, vec![(ids[4], "啊4".to_string())]);
        assert!(latest.more);

        let older = h.page(r"E:\G\a.exe", Some(latest.sentences[0].id), 10);
        assert_eq!(older.sentences.len(), 3, "只取这个游戏、只取更早的");
        assert!(!older.more);
        assert_eq!(h.count(r"E:\G\a.exe"), 5);
    }

    #[test]
    fn ids_never_reuse_after_reopen() {
        let file = std::env::temp_dir().join(format!("fukidashi-history-{}.db", std::process::id()));
        let a = History::open(Some(&file)).add("g", "k", "一", 1).unwrap();
        let b = History::open(Some(&file)).add("g", "k", "二", 2).unwrap();
        assert!(b > a);
        assert!(b < EPHEMERAL_BASE);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", file.display()));
        }
    }
}
