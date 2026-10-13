use crate::backlog::Backlog;
use crate::model::{Event, HookLine, ThreadStat};
use crate::normalize::{collapse_repeats, has_japanese, kana_ratio};
use std::collections::{HashMap, HashSet};

/// 单路来源的累积状态。
struct ThreadState {
    name: String,
    hcode: String,
    count: usize,
    total_len: usize,
    kana_sum: f32,
    seen: HashSet<String>,
    sample: String,
    /// 长度为 1 的行占比统计，用来识别逐字输出
    single_char: usize,
    /// 这一路是否出现过日文。累积记录，避免每次打分都重扫 sample
    saw_japanese: bool,
}

impl ThreadState {
    fn new(name: String, hcode: String) -> Self {
        Self {
            name,
            hcode,
            count: 0,
            total_len: 0,
            kana_sum: 0.0,
            seen: HashSet::new(),
            sample: String::new(),
            single_char: 0,
            saw_japanese: false,
        }
    }

    fn push(&mut self, text: &str) {
        let n = text.chars().count();
        self.count += 1;
        self.total_len += n;
        self.kana_sum += kana_ratio(text);
        if n <= 1 {
            self.single_char += 1;
        }
        if !self.saw_japanese && has_japanese(text) {
            self.saw_japanese = true;
        }
        // seen 只用于估算重复率，容量设上限避免长时间运行后无限增长
        if self.seen.len() < 4096 {
            self.seen.insert(text.to_string());
        }
        if !text.trim().is_empty() {
            self.sample = text.to_string();
        }
    }

    fn per_glyph(&self) -> bool {
        // 一半以上的行只有一个字符，基本可以断定是逐字钩子。
        // 门槛必须和 score_from 的起评线（count >= 3）一致：如果这里要求更多样本，
        // 中间那段窗口里逐字源拿不到惩罚，会抢在正文之前被选中，
        // 于是刚连上的头一两秒界面上全是单个字符。
        self.count >= 3 && self.single_char * 2 > self.count
    }

    fn avg_len(&self) -> f32 {
        if self.count == 0 { 0.0 } else { self.total_len as f32 / self.count as f32 }
    }

    fn kana(&self) -> f32 {
        if self.count == 0 { 0.0 } else { self.kana_sum / self.count as f32 }
    }

    /// 热路径打分输入：不碰任何 String。
    fn score_input(&self) -> ScoreInput {
        ScoreInput {
            count: self.count,
            unique_count: self.seen.len(),
            avg_len: self.avg_len(),
            kana_ratio: self.kana(),
            per_glyph: self.per_glyph(),
            has_jp: self.saw_japanese,
        }
    }

    fn stat(&self, key: &str) -> ThreadStat {
        ThreadStat {
            key: key.to_string(),
            name: self.name.clone(),
            hcode: self.hcode.clone(),
            sample: self.sample.clone(),
            count: self.count,
            unique_count: self.seen.len(),
            avg_len: self.avg_len(),
            kana_ratio: self.kana(),
            score: score_from(self.score_input()),
            per_glyph: self.per_glyph(),
        }
    }
}

/// 打分所需的全部输入。刻意只用标量 —— 打分在每一行到达时都要跑，
/// 逐字钩子每秒能触发上百次，不能在这条路径上做任何堆分配。
struct ScoreInput {
    count: usize,
    unique_count: usize,
    avg_len: f32,
    kana_ratio: f32,
    per_glyph: bool,
    has_jp: bool,
}

/// 给一路来源打分，0~100。分数高的最像"正文"。
///
/// 这是整个产品体验的关键：用户不该被迫去理解 hook 线程是什么概念。
/// 判据全部来自可观测的统计特征，不依赖任何游戏或引擎的先验知识。
fn score_from(s: ScoreInput) -> f32 {
    // 样本太少不下结论，先给一个偏低的分让它待命
    if s.count < 3 {
        return 5.0;
    }
    // 完全不含日文的来源，对日文游戏来说不可能是正文
    if !s.has_jp {
        return 0.0;
    }

    let mut v = 0.0f32;

    // 1) 假名占比：最强的正信号。正文一般在 0.3~0.7 之间
    v += (s.kana_ratio * 100.0).min(45.0);

    // 2) 平均长度：正文一句通常 10~120 字。过短是逐字或标签，过长是整页 dump
    v += match s.avg_len {
        l if l < 2.0 => -25.0,
        l if l < 8.0 => 0.0,
        l if l <= 120.0 => 25.0,
        l if l <= 300.0 => 8.0,
        _ => -10.0,
    };

    // 3) 新鲜度：正文持续产出新句子；菜单和人名框反复输出同样的字符串。
    //    但样本很少时这个比值恒为 1.0，纯属噪声 —— 按样本量线性放开权重，
    //    否则任何刚出现的来源都会凭"全是新内容"拿到满额加分。
    let fresh = if s.count == 0 { 0.0 } else { s.unique_count as f32 / s.count as f32 };
    let confidence = (s.count as f32 / 10.0).min(1.0);
    v += fresh * 25.0 * confidence;

    // 4) 逐字钩子降权 —— 能用整句来源就别用它，重组总会有误差
    if s.per_glyph {
        v -= 20.0;
    }

    // 5) 有一定的量才可信
    v += (s.count as f32).log10().min(2.0) * 5.0;

    v.clamp(0.0, 100.0)
}

/// 预热期：收到这么多行之前不做自动选择。
///
/// 必要性来自一个不对称：逐字钩子每句话会产出几十行，整句来源只产出一行。
/// 所以任何"按样本量"的判据天然偏袒逐字源 —— 在正文攒够三五句之前，
/// 逐字源就已经有几十个样本，会先被选中，界面上先闪一串单个字符再纠正。
/// 与其调权重打补丁，不如老实等够数据再下结论。
const WARMUP_LINES: usize = 60;

/// 文本流水线：喂进原始 hook 行，吐出统计和（可能的）成句文本。
pub struct Pipeline {
    threads: HashMap<String, ThreadState>,
    /// 已选中的来源。None 表示交给自动选择。
    pinned: Option<String>,
    auto: Option<String>,
    /// 全部来源累计收到的行数，用于判断预热是否结束
    total_lines: usize,
    /// 上次为这个游戏手动锁定过的 H-code。key 里带着 pid，每次启动都不同，
    /// H-code 则是相对模块地址、跨启动稳定，所以记的是它：出现时自动锁定。
    prefer: Option<String>,
    /// 挡住打开 backlog / 快进时重绘出来的旧句子
    backlog: Backlog,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            threads: HashMap::new(),
            pinned: None,
            auto: None,
            total_lines: 0,
            prefer: None,
            backlog: Backlog::default(),
        }
    }

    /// 用这个游戏以前读过的句子打底，回看时才认得出旧句子
    pub fn seed(&mut self, lines: Vec<String>) {
        self.backlog.seed(lines);
    }

    pub fn prefer(&mut self, hcode: Option<String>) {
        self.prefer = hcode;
    }

    pub fn pin(&mut self, key: Option<String>) {
        self.pinned = key;
    }

    /// 当前锁定那一路的 H-code，用来记进游戏记录
    pub fn pinned_hcode(&self) -> Option<String> {
        let key = self.pinned.as_ref()?;
        self.threads.get(key).map(|t| t.hcode.clone()).filter(|h| !h.is_empty())
    }

    pub fn active(&self) -> Option<&String> {
        self.pinned.as_ref().or(self.auto.as_ref())
    }

    /// 换源时整体清空。锁定也必须清：旧 key 带着旧 pid，留着的话新会话永远对不上，一句都不显示。
    pub fn reset(&mut self) {
        self.threads.clear();
        self.pinned = None;
        self.prefer = None;
        self.auto = None;
        self.total_lines = 0;
        self.backlog = Backlog::default();
    }

    /// 预热是否还在进行。界面可以据此显示"正在识别来源…"而不是空白。
    pub fn warming_up(&self) -> bool {
        self.pinned.is_none() && self.total_lines < WARMUP_LINES
    }

    /// 吃进一行；若该行应作为正文推给前端，返回**归一化之后**的文本。
    ///
    /// 刻意返回 String 而不是 bool：早先的版本返回 bool，调用方只好自己去用
    /// `line.text`，于是这里做的折叠算完就被丢掉了 —— 界面上照旧是原始重复文本，
    /// 而单元测试只断言返回值为真，完全看不出来。把处理结果本身作为返回值，
    /// 调用方就没有拿到未处理文本的机会了。
    pub fn feed(&mut self, line: &HookLine) -> Option<String> {
        if self.pinned.is_none() && self.prefer.as_deref().is_some_and(|h| h == line.hcode) {
            self.pinned = Some(line.key.clone());
        }
        match self.threads.get_mut(&line.key) {
            Some(t) => t.push(&line.text),
            None => {
                let mut t = ThreadState::new(line.name.clone(), line.hcode.clone());
                t.push(&line.text);
                self.threads.insert(line.key.clone(), t);
            }
        }
        self.total_lines += 1;

        self.recompute_auto();

        let Some(active) = self.active() else { return None };
        if active != &line.key {
            return None;
        }

        let text = collapse_repeats(line.text.trim());
        let text = text.trim();
        if text.is_empty() || !self.backlog.accept(text, std::time::Instant::now()) {
            return None;
        }
        Some(text.to_string())
    }

    /// 重新计算自动选择的来源。
    ///
    /// 加了迟滞：只有当挑战者比现任高出一定幅度时才换，
    /// 否则界面会在两路分数接近时反复横跳。
    fn recompute_auto(&mut self) {
        // 数据不够就先别下结论，见 WARMUP_LINES
        if self.total_lines < WARMUP_LINES {
            return;
        }

        let mut best: Option<(&String, f32)> = None;
        for (k, st) in &self.threads {
            let sc = score_from(st.score_input());
            if best.as_ref().is_none_or(|(_, b)| sc > *b) {
                best = Some((k, sc));
            }
        }
        let Some((best_key, sc)) = best else { return };

        // 先只用借用把"换不换"判出来，确定要换了才付 clone 的代价。
        let switching = match &self.auto {
            None => sc > 20.0,
            // 迟滞：挑战者要明显更好才换，否则两路分数接近时界面会反复横跳
            Some(cur) if cur != best_key => {
                let cur_score =
                    self.threads.get(cur).map(|s| score_from(s.score_input())).unwrap_or(0.0);
                sc > cur_score + 12.0
            }
            _ => false,
        };
        if !switching {
            return;
        }

        self.auto = Some(best_key.clone());
    }

    /// 当前所有来源的统计（按分数降序）和选择状态，推给前端的快照
    pub fn snapshot(&self) -> Event {
        let mut threads: Vec<ThreadStat> = self.threads.iter().map(|(k, st)| st.stat(k)).collect();
        threads.sort_by(|a, b| b.score.total_cmp(&a.score));
        Event::Threads {
            threads,
            active: self.active().cloned(),
            pinned: self.pinned.clone(),
            warming_up: self.warming_up(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(key: &str, text: &str) -> HookLine {
        HookLine {
            key: key.into(),
            name: "test".into(),
            hcode: String::new(),
            text: text.into(),
        }
    }

    #[test]
    fn prefers_prose_over_ascii_noise() {
        let mut p = Pipeline::new();
        // 自造的样例句，不引用任何具体作品的文本
        let prose = [
            "窓の外はもう暗くなっていた。",
            "彼女は静かにカップを置いて、こちらを見た。",
            "少しだけ迷ってから、僕はうなずいた。",
            "明日もまた同じ時間にここへ来よう。",
        ];
        for (i, t) in prose.iter().enumerate() {
            let _ = p.feed(&line("prose", t));
            let _ = p.feed(&line("noise", "es.BT.NAME.SET"));
            let _ = i;
        }
        // 多喂几轮让样本量够
        for t in prose.iter().cycle().take(40) {
            let _ = p.feed(&line("prose", t));
            let _ = p.feed(&line("noise", "es.BT.NAME.SET"));
        }
        assert_eq!(p.active().map(String::as_str), Some("prose"));
    }

    #[test]
    fn pin_overrides_auto() {
        let mut p = Pipeline::new();
        for _ in 0..20 {
            let _ = p.feed(&line("prose", "これはテスト文章です。"));
        }
        p.pin(Some("manual".into()));
        assert_eq!(p.active().map(String::as_str), Some("manual"));
    }

    #[test]
    fn suppresses_consecutive_duplicates() {
        let mut p = Pipeline::new();
        for _ in 0..10 {
            let _ = p.feed(&line("a", "テスト文章がここにあります。"));
        }
        p.pin(Some("a".into()));
        assert!(p.feed(&line("a", "新しい文章です。")).is_some());
        assert!(p.feed(&line("a", "新しい文章です。")).is_none());
        assert!(p.feed(&line("a", "また別の文章です。")).is_some());
    }

    /// 回归保护：`feed` 必须交出**归一化之后**的文本。
    ///
    /// 早先 `feed` 返回 bool，调用方只能自己去读 `line.text`，
    /// 于是折叠算完就被丢掉，界面上照旧是原始重复文本。
    /// 当时的测试只断言返回值为真，对这个 bug 完全免疫。
    #[test]
    fn feed_returns_normalized_text() {
        let mut p = Pipeline::new();
        p.pin(Some("a".into()));
        // 每字重复 5 遍，模拟描边绘制多遍触发的钩子输出
        let raw: String = "きょうはいいてんきだ".chars().flat_map(|c| std::iter::repeat_n(c, 5)).collect();
        assert_eq!(p.feed(&line("a", &raw)).as_deref(), Some("きょうはいいてんきだ"));
    }

    #[test]
    fn ascii_only_source_scores_zero() {
        let mut t = ThreadState::new("n".into(), String::new());
        for _ in 0..50 {
            t.push("SetTextColor");
        }
        assert_eq!(score_from(t.score_input()), 0.0);
    }

    #[test]
    fn prefers_remembered_hcode_and_reset_clears_pin() {
        let line = |key: &str, hcode: &str, text: &str| HookLine {
            key: key.into(),
            name: "h".into(),
            hcode: hcode.into(),
            text: text.into(),
        };
        let mut p = Pipeline::new();
        p.prefer(Some("HQX-C@20DA90:a.exe".into()));
        p.feed(&line("1:aa:0:0", "HB0@0", "ノイズ"));
        assert!(p.pinned.is_none(), "H-code 不符的不锁定");
        let out = p.feed(&line("1:bb:0:0", "HQX-C@20DA90:a.exe", "こんにちは、世界"));
        assert_eq!(p.pinned.as_deref(), Some("1:bb:0:0"));
        assert!(out.is_some(), "锁定后第一句就该出来");
        assert_eq!(p.pinned_hcode().as_deref(), Some("HQX-C@20DA90:a.exe"));

        p.reset();
        assert!(p.pinned.is_none() && p.prefer.is_none());
    }
}
