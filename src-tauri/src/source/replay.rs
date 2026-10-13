use super::Source;
use crate::model::HookLine;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

/// 离线回放源：不需要游戏、不需要钩子，就能驱动整条流水线。
///
/// 它会同时产出四路来源，刻意模拟真实钩取时的噪声环境：
///   prose    —— 整句正文（期望自动选择命中这一路）
///   name     —— 人名框，短且高度重复
///   ui       —— 纯 ASCII 的内部标识符
///   glyph    —— 把正文拆成单字，模拟 GetGlyphOutline 那类逐字钩子
///
/// 可以传入一个 JSON 文件路径来替换 prose 的内容，用于拿真实剧本压测。
/// 文件不进仓库，避免把受版权保护的文本混进项目。
pub struct ReplaySource {
    pub lines: Vec<String>,
    pub interval_ms: u64,
}

/// 内置样例句，全部自造，仅用于让界面在无数据时也能跑起来。
const DEMO: &[&str] = &[
    "窓の外はもう暗くなっていた。",
    "彼女は静かにカップを置いて、こちらを見た。",
    "「まだ帰らないの？」",
    "少しだけ迷ってから、僕はうなずいた。",
    "廊下には誰の姿もなく、足音だけが響いている。",
    "「今日はありがとう。おかげで助かったよ」",
    "そう言うと、彼女はほんの少しだけ笑った。",
    "明日もまた同じ時間にここへ来よう。",
];

const NAMES: &[&str] = &["主人公", "彼女", "店員"];
const UI: &[&str] = &["es.BT.NAME.SET", "SetTextColor", "MainWindow", "btn_ok"];

impl ReplaySource {
    pub fn new(path: Option<&str>, interval_ms: u64) -> Self {
        let lines = path
            .and_then(|p| load(p).ok())
            .filter(|v: &Vec<String>| !v.is_empty())
            .unwrap_or_else(|| DEMO.iter().map(|s| s.to_string()).collect());
        Self { lines, interval_ms: interval_ms.max(50) }
    }
}

/// 支持两种 JSON 形态：一个字符串数组，或 `{"messages":[{"text":"..."}]}`。
/// 后者正是脚本提取工具的输出格式，可以直接拿来当回放素材。
fn load(path: &str) -> Result<Vec<String>> {
    let raw = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;

    if let Some(arr) = v.as_array() {
        return Ok(arr.iter().filter_map(|x| x.as_str().map(str::to_string)).collect());
    }
    if let Some(arr) = v.get("messages").and_then(|m| m.as_array()) {
        return Ok(arr
            .iter()
            .filter_map(|x| x.get("text").and_then(|t| t.as_str()).map(str::to_string))
            .collect());
    }
    Ok(Vec::new())
}

fn line(key: &str, name: &str, text: &str) -> HookLine {
    HookLine {
        key: key.to_string(),
        name: name.to_string(),
        hcode: String::new(),
        text: text.to_string(),
    }
}

impl Source for ReplaySource {
    fn run(self: Box<Self>, tx: Sender<HookLine>, stop: Arc<AtomicBool>) -> Result<()> {
        let gap = Duration::from_millis(self.interval_ms);
        let mut i = 0usize;

        while !stop.load(Ordering::Relaxed) {
            let text = &self.lines[i % self.lines.len()];

            let batch = [
                line("sim:prose", "TextOutW", text),
                line("sim:name", "TextOutW", NAMES[i % NAMES.len()]),
                line("sim:ui", "ExtTextOutA", UI[i % UI.len()]),
            ];
            for l in batch {
                if tx.send(l).is_err() {
                    return Ok(());
                }
            }

            // 逐字来源：同一句拆成单字发出去
            for c in text.chars() {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                if tx.send(line("sim:glyph", "GetGlyphOutlineW", &c.to_string())).is_err() {
                    return Ok(());
                }
            }

            i += 1;
            // 切成小片睡，让停止能及时响应
            let mut left = gap;
            let step = Duration::from_millis(50);
            while left > Duration::ZERO && !stop.load(Ordering::Relaxed) {
                let d = left.min(step);
                std::thread::sleep(d);
                left -= d;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_demo_when_path_missing() {
        let s = ReplaySource::new(Some("definitely/not/here.json"), 100);
        assert_eq!(s.lines.len(), DEMO.len());
    }

    /// 端到端：回放源真正驱动流水线，自动选择要落在整句正文那一路。
    ///
    /// 这条路径就是界面上「先看演示」按钮背后发生的事 —— 单元测试喂的是
    /// 手工构造的行，覆盖不到两个组件接在一起的行为。
    #[test]
    fn replay_drives_pipeline_to_prose() {
        use crate::pipeline::Pipeline;
        use std::sync::mpsc::channel;
        use std::time::Duration as Dur;

        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let src = Box::new(ReplaySource::new(None, 50));

        let stop_worker = stop.clone();
        let worker = std::thread::spawn(move || {
            let _ = src.run(tx, stop_worker);
        });

        let mut p = Pipeline::new();
        let mut emitted: Vec<String> = Vec::new();
        for _ in 0..300 {
            match rx.recv_timeout(Dur::from_secs(5)) {
                Ok(l) => {
                    // 收集 feed 交出的文本本身，而不是原始行 —— 这样这个测试
                    // 也能覆盖归一化，而不只是"选对了来源"
                    if let Some(t) = p.feed(&l) {
                        emitted.push(t);
                    }
                }
                Err(_) => break,
            }
        }
        stop.store(true, Ordering::Relaxed);
        drop(rx);
        let _ = worker.join();

        assert_eq!(p.active().map(String::as_str), Some("sim:prose"), "自动选择应落在正文那一路");
        assert!(!emitted.is_empty(), "应该有正文被推出来");
        // 选对了就该是整句，而不是逐字来源的单个字符
        assert!(emitted.iter().all(|t| t.chars().count() > 3), "推出来的应是整句：{emitted:?}");
    }

    #[test]
    fn parses_messages_shape() {
        let dir = std::env::temp_dir().join("fukidashi_test_replay.json");
        std::fs::write(&dir, r#"{"messages":[{"text":"あ"},{"text":"い"}]}"#).unwrap();
        let s = ReplaySource::new(dir.to_str(), 100);
        assert_eq!(s.lines, vec!["あ", "い"]);
        let _ = std::fs::remove_file(&dir);
    }
}
