//! 自动振假名：分词取读音，再按送假名对齐，只给汉字部分标注。
//!
//! 手法沿用 audoria 的歌词注音：kuromoji 同源的 IPADIC 词典分词（这里用其 Rust 移植 lindera），
//! 以词里的假名片段为锚点把读音切给各段汉字（取り消し + とりけし → 取(と)り消(け)し），
//! 分词器会拆错的熟字训整词查表。

use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RubySegment {
    pub text: String,
    /// 平假名读音；不需要标注的片段为空
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ruby: Option<String>,
}

impl RubySegment {
    fn plain(text: &str) -> Self {
        Self { text: text.to_string(), ruby: None }
    }
    fn ruby(text: &str, ruby: &str) -> Self {
        Self { text: text.to_string(), ruby: Some(ruby.to_string()) }
    }
}

/// 熟字训：分词器会把它们拆成逐字的音读或数词（二人 → に+にん），整词查表。
const WORD_READINGS: &[(&str, &str)] = &[
    ("一人", "ひとり"),
    ("二人", "ふたり"),
    ("今日", "きょう"),
    ("明日", "あした"),
    ("昨日", "きのう"),
    ("大人", "おとな"),
    ("今年", "ことし"),
    ("今朝", "けさ"),
    ("一日", "いちにち"),
];

/// 单独成词时 IPADIC 容易读成前缀或音读的字
const READING_OVERRIDES: &[(&str, &str)] = &[("今", "いま")];

fn segmenter() -> Option<&'static Segmenter> {
    static SEG: OnceLock<Option<Segmenter>> = OnceLock::new();
    SEG.get_or_init(|| {
        let dict = load_dictionary("embedded://ipadic")
            .inspect_err(|e| println!("[rust] 加载 IPADIC 失败，振假名不可用: {e}"))
            .ok()?;
        Some(Segmenter::new(Mode::Normal, dict, None))
    })
    .as_ref()
}

/// 提前加载词典，免得第一句正文等它
pub fn warm_up() {
    std::thread::spawn(|| {
        segmenter();
    });
}

fn is_kanji(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' | '々' | '〆' | 'ヶ')
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{309F}' | '\u{30A1}'..='\u{30FA}')
}

/// 片假名 ァ(U+30A1)..ヶ(U+30F6) 恰好比对应的平假名大 0x60
fn to_hiragana(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// 只标注含汉字、且像日文（有假名）的句子。纯汉字的多半是中文或菜单，标了反而错。
pub fn annotate(text: &str) -> Option<Vec<RubySegment>> {
    if !text.chars().any(is_kanji) || !text.chars().any(is_kana) {
        return None;
    }
    let seg = segmenter()?;
    Some(merge_plain(annotate_words(text, seg)))
}

fn annotate_words(text: &str, seg: &Segmenter) -> Vec<RubySegment> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        // 找最早出现的熟字训
        let hit = WORD_READINGS
            .iter()
            .filter_map(|(w, r)| rest.find(w).map(|i| (i, *w, *r)))
            .min_by_key(|(i, _, _)| *i);
        match hit {
            Some((i, w, r)) => {
                out.extend(annotate_tokens(&rest[..i], seg));
                out.push(RubySegment::ruby(w, r));
                rest = &rest[i + w.len()..];
            }
            None => {
                out.extend(annotate_tokens(rest, seg));
                break;
            }
        }
    }
    out
}

fn annotate_tokens(text: &str, seg: &Segmenter) -> Vec<RubySegment> {
    if text.is_empty() {
        return Vec::new();
    }
    let Ok(mut tokens) = seg.segment(Cow::Borrowed(text)) else {
        return vec![RubySegment::plain(text)];
    };
    let mut out = Vec::new();
    let mut cursor = 0;
    for token in tokens.iter_mut() {
        let surface = token.surface.as_ref().to_string();
        // 分词器会吞掉空白；片段必须能原样拼回整句，所以把间隙补回去
        if let Some(pos) = text[cursor..].find(&surface) {
            if pos > 0 {
                out.push(RubySegment::plain(&text[cursor..cursor + pos]));
            }
            cursor += pos + surface.len();
        }
        if !surface.chars().any(is_kanji) {
            out.push(RubySegment::plain(&surface));
            continue;
        }
        let reading = READING_OVERRIDES
            .iter()
            .find(|(w, _)| *w == surface)
            .map(|(_, r)| r.to_string())
            .or_else(|| {
                // IPADIC 详情：品詞,細分類1,2,3,活用型,活用形,原形,読み,発音
                let details = token.details();
                details.get(7).filter(|r| **r != "*").map(|r| to_hiragana(r))
            });
        match reading {
            Some(r) => out.extend(align(&surface, &r)),
            None => out.push(RubySegment::plain(&surface)),
        }
    }
    if cursor < text.len() {
        out.push(RubySegment::plain(&text[cursor..]));
    }
    out
}

/// 以假名片段为锚点，把读音分给各段汉字：取り消し + とりけし → 取(と) り 消(け) し。
/// 对不上（读音与字面不一致的特殊读法）就整词标注。
fn align(surface: &str, reading: &str) -> Vec<RubySegment> {
    let runs = split_runs(surface);
    let reading: Vec<char> = reading.chars().collect();
    let mut cuts = Vec::new();
    if match_runs(&runs, 0, &reading, 0, &mut cuts) {
        runs.iter()
            .zip(cuts)
            .map(|((text, kanji), r)| if *kanji { RubySegment::ruby(text, &r) } else { RubySegment::plain(text) })
            .collect()
    } else {
        vec![RubySegment::ruby(surface, &reading.iter().collect::<String>())]
    }
}

/// 把字面切成交替的 (片段, 是否汉字)
fn split_runs(s: &str) -> Vec<(String, bool)> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    for c in s.chars() {
        let k = is_kanji(c);
        match runs.last_mut() {
            Some((text, kanji)) if *kanji == k => text.push(c),
            _ => runs.push((c.to_string(), k)),
        }
    }
    runs
}

/// 回溯匹配：假名片段必须与读音逐字吻合，汉字片段至少吃一个读音字符，优先吃得少。
fn match_runs(runs: &[(String, bool)], i: usize, reading: &[char], pos: usize, cuts: &mut Vec<String>) -> bool {
    let Some((text, kanji)) = runs.get(i) else {
        return pos == reading.len();
    };
    if *kanji {
        for end in pos + 1..=reading.len() {
            cuts.push(reading[pos..end].iter().collect());
            if match_runs(runs, i + 1, reading, end, cuts) {
                return true;
            }
            cuts.pop();
        }
        false
    } else {
        let kana: Vec<char> = to_hiragana(text).chars().collect();
        if reading[pos..].starts_with(&kana) {
            cuts.push(String::new());
            if match_runs(runs, i + 1, reading, pos + kana.len(), cuts) {
                return true;
            }
            cuts.pop();
        }
        false
    }
}

/// 相邻的无注音片段合并成一段，减少前端节点数
fn merge_plain(segments: Vec<RubySegment>) -> Vec<RubySegment> {
    let mut out: Vec<RubySegment> = Vec::new();
    for s in segments {
        match out.last_mut() {
            Some(prev) if prev.ruby.is_none() && s.ruby.is_none() => prev.text.push_str(&s.text),
            _ => out.push(s),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(segs: &[RubySegment]) -> String {
        segs.iter()
            .map(|s| match &s.ruby {
                Some(r) => format!("{}({r})", s.text),
                None => s.text.clone(),
            })
            .collect()
    }

    #[test]
    fn aligns_okurigana() {
        assert_eq!(render(&align("取り消し", "とりけし")), "取(と)り消(け)し");
        assert_eq!(render(&align("食べる", "たべる")), "食(た)べる");
    }

    #[test]
    fn falls_back_to_whole_word() {
        assert_eq!(render(&align("大人しい", "おとなしかった")), "大人しい(おとなしかった)");
    }

    #[test]
    fn annotates_sentence_and_keeps_text_intact() {
        let text = "彼女は静かにカップを置いて、こちらを見た。";
        let segs = annotate(text).unwrap();
        assert_eq!(segs.iter().map(|s| s.text.as_str()).collect::<String>(), text);
        assert_eq!(render(&segs), "彼女(かのじょ)は静(しず)かにカップを置(お)いて、こちらを見(み)た。");
    }

    #[test]
    fn uses_jukujikun_table() {
        let segs = annotate("今日は二人で帰ろう").unwrap();
        assert!(render(&segs).starts_with("今日(きょう)は二人(ふたり)で帰(かえ)ろう"));
    }

    #[test]
    fn keeps_whitespace() {
        let text = "窓の外 はもう暗い";
        let segs = annotate(text).unwrap();
        assert_eq!(segs.iter().map(|s| s.text.as_str()).collect::<String>(), text);
    }

    #[test]
    fn skips_lines_without_kana() {
        assert!(annotate("关西国际机场").is_none());
        assert!(annotate("ありがとう").is_none());
    }
}
