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
//!
//! 规则全部从真机记录里归纳，见 tests 里的样本。

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// 最近一次认出回看后，这么久之内都按回看处理
const BACKLOG_WINDOW: Duration = Duration::from_secs(15);
/// 记住这么多句读过的，用来认出旧句子
const MEMORY: usize = 3000;

#[derive(Default)]
pub struct Backlog {
    /// 读过的句子，按时间顺序
    seen: Vec<String>,
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
            self.index = self.seen.iter().enumerate().map(|(i, s)| (s.clone(), i)).collect();
        }
        self.index.insert(text.clone(), self.seen.len());
        self.seen.push(text);
    }

    /// 返回应当显示的文本（可能修掉了一段重复），None 表示这是回看重绘，丢掉
    pub fn filter(&mut self, text: &str, now: Instant) -> Option<String> {
        let in_backlog = self.last_drop.is_some_and(|t| now - t < BACKLOG_WINDOW);
        let chars: Vec<char> = text.chars().collect();
        let reps = repeats(&chars);

        // 只有一处长重复：是单句的重绘毛刺（最后一句话画了两遍），修掉而不是扔掉
        let text: String = match reps.as_slice() {
            [(i, len)] if *len >= 8 => chars[..i + len].iter().chain(&chars[i + 2 * len..]).collect(),
            _ => text.to_string(),
        };
        let single_fixed = matches!(reps.as_slice(), [(_, len)] if *len >= 8);

        // 一行里拼了好几句：多处重复、三段以上的台词、或者含两句以上读过的长句
        let dump = (!single_fixed && reps.len() >= 2)
            || text.matches('「').count() >= 3
            || self.contains_seen(&text, 10, 2);
        if dump {
            return self.reject(now);
        }

        if let Some(&j) = self.index.get(&text) {
            let reversed = self.last_dup.is_some_and(|(i, t)| i > j && now - t < BACKLOG_WINDOW);
            self.last_dup = Some((j, now));
            if in_backlog || reversed {
                return self.reject(now);
            }
        } else {
            // 回看状态里，带着重复或夹着读过句子的残片也是重绘出来的
            if in_backlog && (!reps.is_empty() && !single_fixed || self.contains_seen(&text, 6, 1)) {
                return self.reject(now);
            }
            self.last_dup = None;
        }
        self.remember(text.clone());
        Some(text)
    }

    fn reject(&mut self, now: Instant) -> Option<String> {
        self.last_drop = Some(now);
        None
    }

    /// 文本里是否含有至少 `n` 句不同的、不短于 `min` 字的读过的句子（且不是整句相同）
    fn contains_seen(&self, text: &str, min: usize, n: usize) -> bool {
        let len = text.chars().count();
        if len < min * n {
            return false;
        }
        let mut hits = HashSet::new();
        for s in self.seen.iter().rev().take(MEMORY) {
            if s != text && s.chars().count() >= min && text.contains(s.as_str()) {
                hits.insert(s.as_str());
                if hits.len() >= n {
                    return true;
                }
            }
        }
        false
    }
}

/// 找出所有「一段紧接着又出现一遍」的位置 (起点, 长度)，互不重叠。
///
/// 重复单元至少 4 个字、4 种不同的字：`はぁ……はぁ……`、`ドキドキ` 这种叠字
/// 和语气是正文本来就有的，不算。
fn repeats(c: &[char]) -> Vec<(usize, usize)> {
    const MAX_UNIT: usize = 160;
    let n = c.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 8 <= n {
        let hit = (4..=MAX_UNIT.min((n - i) / 2)).find(|&len| {
            c[i..i + len] == c[i + len..i + 2 * len] && c[i..i + len].iter().collect::<HashSet<_>>().len() >= 4
        });
        match hit {
            Some(len) => {
                out.push((i, len));
                i += 2 * len;
            }
            None => i += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(b: &mut Backlog, lines: &[&str]) -> Vec<Option<String>> {
        let t0 = Instant::now();
        lines.iter().enumerate().map(|(k, l)| b.filter(l, t0 + Duration::from_millis(1500 * k as u64))).collect()
    }

    #[test]
    fn story_repeats_pass() {
        let mut b = Backlog::default();
        let out = feed(&mut b, &["蓮「旦那様」", "千明「おかえり、蓮さん」", "蓮「旦那様」", "蓮「はぁ……はぁ……んっ……はぁ…………はぁ……」"]);
        assert!(out.iter().all(Option::is_some), "{out:?}");
    }

    #[test]
    fn doubled_last_sentence_is_repaired() {
        let mut b = Backlog::default();
        let out = feed(&mut b, &["昨日値付けした本を、店の書棚へと並べていく。蓮さんのおかげで、古い絶版本が結構な数発掘された。蓮さんのおかげで、古い絶版本が結構な数発掘された。"]);
        assert_eq!(out[0].as_deref(), Some("昨日値付けした本を、店の書棚へと並べていく。蓮さんのおかげで、古い絶版本が結構な数発掘された。"));
    }

    #[test]
    fn backlog_redraw_is_dropped() {
        let mut b = Backlog::default();
        b.seed(["忍「いいかい、世の中には謎がたくさんあるんだ」", "千明「わ、分かりましたよ」", "忍「ふむ。理解が早くてよろしい」"].map(String::from));
        let out = feed(&mut b, &[
            // 打开 backlog：一屏旧句子，每句画两遍
            "蓮「…………」蓮「…………」蓮さんが頬を差し出してくる。蓮さんが頬を差し出してくる。",
            // 余下的散句：读过的、带重复的残片
            "蓮さんは律義にお辞儀した。蓮「恐縮です」蓮「恐縮です」",
            "忍「ふむ。理解が早くてよろしい」",
            "忍「いいかい、世の中には謎がたくさんあるんだ」",
        ]);
        assert!(out.iter().all(Option::is_none), "{out:?}");
    }

    #[test]
    fn scrolling_back_line_by_line_is_dropped() {
        let mut b = Backlog::default();
        b.seed(["一。あいうえお", "二。かきくけこ", "三。さしすせそ", "四。たちつてと"].map(String::from));
        // 倒着来的读过的句子：第一句无从判断，第二句起认出来
        let out = feed(&mut b, &["四。たちつてと", "三。さしすせそ", "二。かきくけこ", "五。なにぬねの"]);
        assert_eq!(out[1], None);
        assert_eq!(out[2], None);
        assert_eq!(out[3].as_deref(), Some("五。なにぬねの"), "回看结束后的新句照常显示");
    }

    #[test]
    fn rereading_after_load_passes() {
        let mut b = Backlog::default();
        b.seed(["一。あいうえお", "二。かきくけこ", "三。さしすせそ"].map(String::from));
        // 读档后顺着重读：读过的，但顺序是正的
        let out = feed(&mut b, &["一。あいうえお", "二。かきくけこ", "三。さしすせそ"]);
        assert!(out.iter().all(Option::is_some), "{out:?}");
    }
}
