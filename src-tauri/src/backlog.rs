//! 挡住「回看」类的假新句。
//!
//! 打开游戏的 backlog、快进跳过、读档时，引擎会一口气把很多旧句子重新画一遍。
//! 钩子照单全收，LunaHook 又把 500ms 内到达的文本拼成一行，于是收到的是：
//!
//! - 一大串旧句子拼在一起，通常每句还画了两遍：`A「…」A「…」B。B。…`
//! - 或者一句一句来，但全是读过的，而且顺序倒着（往上翻 backlog）
//!
//! 这些都不该当成新台词显示、翻译、入库。但故事里本来就会重复台词
//! （同一句「旦那様」、回忆里重放的原话），所以不能见到读过的就扔，
//! 要看上下文：先认出一次明确的「批量重绘」，进入回看状态，状态里的重复才扔。
//! 唯一无条件扔掉的是和上一句完全相同的：那是钩子对同一句的重复触发。
//!
//! 快速翻页时相邻几句新台词也会被 LunaHook 拼成一行，形状和回看重绘一模一样
//! （每句画两遍：`A A B B`）。两者的区别在内容：回看重绘的那一屏里一定有刚读过的句子，
//! 快速翻页拼出来的则全是新句。所以拼接行先去重、拆成单句，全是新句且句数不多就逐句放行。
//!
//! 单句里画了两遍的毛刺由 normalize 在这之前修好，这里只判断整行的去留。
//!
//! 规则全部从真机记录里归纳，见 tests 里的样本。

use crate::normalize::repeats;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// 最近一次认出回看后，这么久之内都按回看处理
const BACKLOG_WINDOW: Duration = Duration::from_secs(15);
/// 拼接行拆开后不超过这么多句、且全是新句，才当作快速翻页拼起来的；再多就是整屏重绘
const FAST_MAX: usize = 4;
/// 记住这么多句读过的，用来认出旧句子；连上游戏时也按这个数从历史里取
pub const MEMORY: usize = 3000;

#[derive(Default)]
pub struct Backlog {
    /// 读过的句子及其字数，按时间顺序
    seen: Vec<(String, usize)>,
    /// 句子 → 在 seen 里最后一次出现的位置
    index: HashMap<String, usize>,
    /// 上一句是重复句时，它重复的是第几句；连续倒着重复就是在往上翻 backlog
    last_dup: Option<(usize, Instant)>,
    last_drop: Option<Instant>,
}

impl Backlog {
    /// 用这个游戏以前读过的句子打底：读档后的回看翻到的多半是上次会话的句子
    pub fn seed(&mut self, lines: impl IntoIterator<Item = String>) {
        *self = Self::default();
        for l in lines {
            self.remember(l);
        }
    }

    fn remember(&mut self, text: String) {
        if self.seen.len() >= MEMORY * 2 {
            // 偶尔整体截一半，比每句都挪动便宜
            self.seen.drain(..MEMORY);
            self.index = self.seen.iter().enumerate().map(|(i, (s, _))| (s.clone(), i)).collect();
        }
        self.index.insert(text.clone(), self.seen.len());
        let len = text.chars().count();
        self.seen.push((text, len));
    }

    /// 这一行里应当显示的句子；空表示是重复触发或回看重绘，整行丢掉
    pub fn accept(&mut self, text: &str, now: Instant) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let reps = repeats(&chars);

        // 一行里拼了好几句：多处重复、三段以上的台词、或者含两句以上读过的长句
        let multi = reps.len() >= 2 || text.matches('「').count() >= 3 || self.contains_seen(text, 10, 2);
        if !multi {
            return if self.accept_one(text, &reps, now) { vec![text.to_string()] } else { vec![] };
        }
        let parts = split_sentences(&dedupe(&chars, &reps));
        let fresh = |p: &String| !self.index.contains_key(p) && !self.contains_seen(p, 6, 1);
        if parts.is_empty() || parts.len() > FAST_MAX || !parts.iter().all(fresh) {
            self.reject(now);
            return vec![];
        }
        // 快速翻页拼起来的几句新台词，逐句放行
        parts.into_iter().filter(|p| self.accept_one(p, &[], now)).collect()
    }

    /// 单独一句的去留
    fn accept_one(&mut self, text: &str, reps: &[(usize, usize)], now: Instant) -> bool {
        let in_backlog = self.last_drop.is_some_and(|t| now - t < BACKLOG_WINDOW);

        if let Some(&j) = self.index.get(text) {
            let reversed = self.last_dup.is_some_and(|(i, t)| i > j && now - t < BACKLOG_WINDOW);
            self.last_dup = Some((j, now));
            // 和上一句完全相同：钩子对同一句的重复触发，丢掉但不算回看
            if j + 1 == self.seen.len() {
                return false;
            }
            if in_backlog || reversed {
                return self.reject(now);
            }
        } else {
            // 回看状态里，带着重复或夹着读过句子的残片也是重绘出来的
            if in_backlog && (!reps.is_empty() || self.contains_seen(text, 6, 1)) {
                return self.reject(now);
            }
            self.last_dup = None;
        }
        self.remember(text.to_string());
        true
    }

    fn reject(&mut self, now: Instant) -> bool {
        self.last_drop = Some(now);
        false
    }

    /// 文本里是否含有至少 `n` 句不同的、不短于 `min` 字的读过的句子（且不是整句相同）
    fn contains_seen(&self, text: &str, min: usize, n: usize) -> bool {
        let len = text.chars().count();
        if len < min * n {
            return false;
        }
        let mut hits = HashSet::new();
        for (s, chars) in self.seen.iter().rev().take(MEMORY) {
            if *chars >= min && *chars < len && text.contains(s.as_str()) {
                hits.insert(s.as_str());
                if hits.len() >= n {
                    return true;
                }
            }
        }
        false
    }
}

/// 去掉每处「紧接着又出现一遍」的第二遍
fn dedupe(c: &[char], reps: &[(usize, usize)]) -> Vec<char> {
    let mut out = Vec::with_capacity(c.len());
    let mut k = 0;
    for &(i, len) in reps {
        out.extend_from_slice(&c[k..i + len]);
        k = i + 2 * len;
    }
    out.extend_from_slice(&c[k..]);
    out
}

/// 按句末切开：括号外的句号、问叹号，以及收尾的」』。紧跟在后面的标点归前一句
fn split_sentences(c: &[char]) -> Vec<String> {
    let is_end = |ch: char| matches!(ch, '」' | '』' | '。' | '！' | '？' | '!' | '?');
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for (k, &ch) in c.iter().enumerate() {
        cur.push(ch);
        match ch {
            '「' | '『' => depth += 1,
            '」' | '』' => depth = (depth - 1).max(0),
            _ => {}
        }
        let next_is_tail = c.get(k + 1).is_some_and(|&n| is_end(n) || matches!(n, '…' | '～' | '♪'));
        if depth == 0 && is_end(ch) && !next_is_tail {
            out.push(std::mem::take(&mut cur));
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(b: &mut Backlog, lines: &[&str]) -> Vec<bool> {
        let t0 = Instant::now();
        lines.iter().enumerate().map(|(k, l)| !b.accept(l, t0 + Duration::from_millis(1500 * k as u64)).is_empty()).collect()
    }

    #[test]
    fn story_repeats_pass() {
        let mut b = Backlog::default();
        let out = feed(&mut b, &["蓮「旦那様」", "千明「おかえり、蓮さん」", "蓮「旦那様」", "蓮「はぁ……はぁ……んっ……はぁ…………はぁ……」"]);
        assert!(out.iter().all(|&x| x), "{out:?}");
    }

    #[test]
    fn same_as_previous_is_dropped() {
        let mut b = Backlog::default();
        b.seed(["一。あいうえお"].map(String::from));
        assert_eq!(feed(&mut b, &["一。あいうえお", "二。かきくけこ", "二。かきくけこ", "三。さしすせそ"]), [false, true, false, true]);
    }

    #[test]
    fn backlog_redraw_is_dropped() {
        let mut b = Backlog::default();
        // 打开 backlog 时重绘的那一屏，最底下就是刚读过的句子
        b.seed(["忍「いいかい、世の中には謎がたくさんあるんだ」", "千明「わ、分かりましたよ」", "忍「ふむ。理解が早くてよろしい」", "蓮さんが頬を差し出してくる。"].map(String::from));
        let out = feed(&mut b, &[
            // 打开 backlog：一屏旧句子，每句画两遍
            "蓮「…………」蓮「…………」蓮さんが頬を差し出してくる。蓮さんが頬を差し出してくる。",
            // 余下的散句：读过的、带重复的残片
            "蓮さんは律義にお辞儀した。蓮「恐縮です」蓮「恐縮です」",
            "忍「ふむ。理解が早くてよろしい」",
            "忍「いいかい、世の中には謎がたくさんあるんだ」",
        ]);
        assert!(out.iter().all(|&x| !x), "{out:?}");
    }

    #[test]
    fn scrolling_back_line_by_line_is_dropped() {
        let mut b = Backlog::default();
        b.seed(["一。あいうえお", "二。かきくけこ", "三。さしすせそ", "四。たちつてと"].map(String::from));
        let out = feed(&mut b, &["四。たちつてと", "三。さしすせそ", "二。かきくけこ", "五。なにぬねの"]);
        // 第一句和读过的最后一句相同，按重复触发丢掉；之后倒着来的认出是回看
        assert_eq!(out, [false, false, false, true], "回看结束后的新句照常显示");
    }

    #[test]
    fn fast_paging_splits_merged_new_lines() {
        let mut b = Backlog::default();
        b.seed(["一。あいうえお", "二。かきくけこ"].map(String::from));
        let t = Instant::now();
        // 两句新台词在 500ms 内到达、每句画了两遍，被拼成一行
        let out = b.accept("千明「おはよう、蓮さん」千明「おはよう、蓮さん」蓮「おはようございます、旦那様」蓮「おはようございます、旦那様」", t);
        assert_eq!(out, ["千明「おはよう、蓮さん」", "蓮「おはようございます、旦那様」"]);
        // 三句没画两遍的也一样拆开
        let out = b.accept("忍「やあ」千明「どうも」蓮さんは黙って頷いた。蓮「……はい」", t + Duration::from_secs(2));
        assert_eq!(out, ["忍「やあ」", "千明「どうも」", "蓮さんは黙って頷いた。", "蓮「……はい」"]);
        // 之后的新句照常，没被当成回看
        assert_eq!(b.accept("三。さしすせそ", t + Duration::from_secs(3)), ["三。さしすせそ"]);
    }

    #[test]
    fn merged_line_with_read_lines_is_redraw() {
        let mut b = Backlog::default();
        b.seed(["千明「おはよう、蓮さん」"].map(String::from));
        let t = Instant::now();
        let out = b.accept("千明「おはよう、蓮さん」千明「おはよう、蓮さん」蓮「おはようございます、旦那様」蓮「おはようございます、旦那様」", t);
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn rereading_after_load_passes() {
        let mut b = Backlog::default();
        b.seed(["一。あいうえお", "二。かきくけこ", "三。さしすせそ"].map(String::from));
        // 读档后顺着重读：读过的，但顺序是正的
        let out = feed(&mut b, &["一。あいうえお", "二。かきくけこ", "三。さしすせそ"]);
        assert!(out.iter().all(|&x| x), "{out:?}");
    }
}
