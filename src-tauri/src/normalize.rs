//! 原始钩子文本的重复折叠。
//!
//! Textractor 的图形界面默认挂着 Remove Repeated Characters / Phrases 这几个
//! 插件，而 CLI 不加载任何 xdll —— 我们拿到的是完全未过滤的原始输出，
//! 重复得自己收拾。
//!
//! 这里所有规则都遵循同一条原则：**只在模式覆盖整个字符串时才动手**。
//! 局部匹配一律不碰，否则会误伤正文里本来就有的叠字（「そそる」「まさか」
//! 里的连续假名、拟声词、长音记号等等）。

/// 折叠整串级别的重复。不匹配任何模式时原样返回。
pub fn collapse_repeats(s: &str) -> String {
    let mut cur: Vec<char> = s.chars().collect();
    // 两种模式可能叠加（先逐字 N 倍重复、整体又重复一遍），迭代到不动点为止。
    // 设上限纯粹是防御性的，正常两三轮就收敛。
    for _ in 0..8 {
        let next = collapse_runs(&cur).unwrap_or_else(|| cur.clone());
        let next = collapse_prefix(&next).unwrap_or(next);
        let next = collapse_echo(&next).unwrap_or(next);
        if next == cur {
            break;
        }
        cur = next;
    }
    cur.into_iter().collect()
}

/// 逐字 N 倍重复：`あああああいいいいい…` → `あい…`。
///
/// 成因是描边文字：同一字形要绘制多遍（四周偏移画描边 + 中间填充），
/// 每遍都触发一次钩子，于是每个字符连续出现 N 次。N 等于该引擎的
/// 绘制遍数，不同游戏不同，不能硬编码 —— 只能从游程长度里估出来。
///
/// **不能用全体游程的最大公约数来估 N。** 那是这条规则早先的做法，
/// 在真机上几乎必然失效：一句话里只要有一个字符没跟着重复 —— 句尾的
/// 「。」走的不是同一条绘制路径、最后一个字的描边被下一次钩子截断、
/// 中间夹了个半角空格 —— 那一个长度为 1 的游程就会把 gcd 拉回 1，
/// **整行原样放行**。也就是说，越是正常的一句话（几乎都带标点），
/// 越是折不掉，界面上看到的仍是满屏重复字。
///
/// 改成投票：候选 N 取所有出现过的游程长度，选出能整除大多数游程的那个。
/// 不合群的游程（那个孤零零的标点）按四舍五入重建，至少保留一个字符。
///
/// 关键细节：原文里本来就连续重复的字符（如 `あいいう` 中的 `いい`），
/// 其游程长度是 2N 而不是 N，天然能被 N 整除，除以 N 后重复得以保留。
///
/// 防误伤门槛（宁可漏折，不可误折）：
/// - N 必须 >= 2；
/// - 能被 N 整除的游程至少要有 `MIN_RUNS` 段 —— 只有三两段时，
///   `ははは、ふふふ、へへへ` 这种连续笑声和描边重复在结构上无法区分；
/// - 这些游程还要覆盖全串大部分字符（见 `COVER_*`）；
/// - 总长度也要达到下限（见 `MIN_TOTAL_LEN`），短串证据不足。
///
/// 长度为 1 的游程永远不被任何 N >= 2 整除，所以正常散文（游程几乎全是 1）
/// 无论如何都凑不够覆盖率，这条规则对它天然免疫。
const MIN_RUNS: usize = 4;
const MIN_TOTAL_LEN: usize = 8;
/// 能整除的游程需覆盖的字符比例：4/5。
const COVER_NUM: usize = 4;
const COVER_DEN: usize = 5;

fn collapse_runs(c: &[char]) -> Option<Vec<char>> {
    if c.len() < MIN_TOTAL_LEN {
        return None;
    }
    // 游程编码：连续相同字符合并为 (字符, 长度)
    let mut runs: Vec<(char, usize)> = Vec::new();
    for &ch in c {
        match runs.last_mut() {
            Some((prev, len)) if *prev == ch => *len += 1,
            _ => runs.push((ch, 1)),
        }
    }
    if runs.len() < MIN_RUNS {
        return None;
    }
    let n = multiplicity(&runs)?;
    // 每段长度除以 N 重建。四舍五入而非整除：绘制被截断的那段（长度不到 N）
    // 也要留下一个字符，否则句尾的字会被整个吞掉。
    let mut out: Vec<char> = Vec::with_capacity(c.len() / n + runs.len());
    for &(ch, len) in &runs {
        let k = ((len + n / 2) / n).max(1);
        out.extend(std::iter::repeat(ch).take(k));
    }
    if out.len() == c.len() {
        return None;
    }
    Some(out)
}

/// 从游程长度里估出绘制遍数 N，估不出来就返回 None。
///
/// 候选只取实际出现过的游程长度（>= 2），从小到大试第一个通过门槛的：
/// 游程序列 `[5, 10, 5, 5]` 里 5 全中而 10 只中一段，投票自然选出 5。
fn multiplicity(runs: &[(char, usize)]) -> Option<usize> {
    let total: usize = runs.iter().map(|&(_, len)| len).sum();
    let mut cands: Vec<usize> = runs.iter().map(|&(_, len)| len).filter(|&l| l >= 2).collect();
    cands.sort_unstable();
    cands.dedup();

    for n in cands {
        let (segments, covered) = runs
            .iter()
            .map(|&(_, len)| len)
            .filter(|len| len % n == 0)
            .fold((0usize, 0usize), |(s, c), len| (s + 1, c + len));
        if segments < MIN_RUNS {
            continue;
        }
        if covered * COVER_DEN >= total * COVER_NUM {
            return Some(n);
        }
    }
    None
}

/// 开头的一段紧接着又出现了一遍：`おはよう…おはよう…続き` → `おはよう…続き`。
///
/// 覆盖整串恰好是 N 份的情况（每折掉一份后剩下的仍以重复开头，迭代收敛），
/// 也覆盖后面还跟着别的内容的情况 —— 后者在真机上很常见：钩子把上一句的
/// 尾巴和新句子拼在同一行里发出来。两份之间允许夹空白，因为不少引擎
/// 会在重发时补一个半角空格。
///
/// **重复单元必须足够长**，这条限制是必需的。
///
/// 日语里叠词是构词法的一部分，且能叠加强度：`ドキドキ`（单元 2）、
/// `ドキドキドキドキ`（单元 4）都是完全正常的词。它们和「一段内容被
/// 重复输出了一遍」在结构上**无法区分** —— 光靠调门槛只是把误伤推到
/// 更大的尺度上，不解决问题。
///
/// 出路是承认这个模糊性，然后押在不对称的代价上：漏折一次只是文本里
/// 多一份重复，用户看得懂；误折一次是把作品原文改错，用户看到的是
/// 错误的内容却毫不知情。所以门槛取到自然叠词几乎不可能达到的长度。
///
/// 真实场景里被重复的是整句话，长度远超这个值，所以并不会漏掉实际问题。
const MIN_UNIT: usize = 8;

fn collapse_prefix(c: &[char]) -> Option<Vec<char>> {
    let n = c.len();
    if n < MIN_UNIT * 2 {
        return None;
    }
    for p in MIN_UNIT..=n / 2 {
        // 两份之间可能夹着重发时补的空白
        let mut q = p;
        while q < n && c[q].is_whitespace() {
            q += 1;
        }
        if q + p > n {
            break;
        }
        if c[..p] == c[q..q + p] {
            let mut out = c[..p].to_vec();
            out.extend_from_slice(&c[q + p..]);
            return Some(out);
        }
    }
    None
}

/// 整句前面多了一段它自己的结尾：`「台词」千明「台词」` → `千明「台词」`，
/// `…から、頬へのキスだ。さすがに…から、頬へのキスだ。` → 后半整句。
///
/// 成因是引擎分两次画同一句（先不带名字、再带名字，或者先画到一半），
/// 两次相隔不到 LunaHook 的合并间隔，被拼成了一行。
/// 前缀同样要不短于 `MIN_UNIT`，理由同上。
fn collapse_echo(c: &[char]) -> Option<Vec<char>> {
    let n = c.len();
    (MIN_UNIT..=n / 2).find(|&k| c[k..].ends_with(&c[..k])).map(|k| c[k..].to_vec())
}

/// 判断一段文本里假名的占比。正文几乎必然含大量平假名，
/// 而菜单、文件名、变量名这些噪声来源通常是纯 ASCII 或纯汉字。
pub fn kana_ratio(s: &str) -> f32 {
    let mut kana = 0usize;
    let mut total = 0usize;
    for c in s.chars() {
        if c.is_whitespace() {
            continue;
        }
        total += 1;
        let u = c as u32;
        // 平假名 3040-309F，片假名 30A0-30FF，半角片假名 FF66-FF9D
        if (0x3040..=0x309F).contains(&u) || (0x30A0..=0x30FF).contains(&u) || (0xFF66..=0xFF9D).contains(&u) {
            kana += 1;
        }
    }
    if total == 0 { 0.0 } else { kana as f32 / total as f32 }
}

/// 是否含日文字符（假名或汉字）。用来把纯 ASCII 的噪声源直接判死。
pub fn has_japanese(s: &str) -> bool {
    s.chars().any(|c| {
        let u = c as u32;
        (0x3040..=0x30FF).contains(&u)      // 假名
            || (0x4E00..=0x9FFF).contains(&u)   // CJK 统一汉字
            || (0xFF66..=0xFF9D).contains(&u)   // 半角片假名
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_echo_before_full_line() {
        assert_eq!(
            collapse_repeats("「もしかして、新本では買えないような本を探してる？」千明「もしかして、新本では買えないような本を探してる？」"),
            "千明「もしかして、新本では買えないような本を探してる？」"
        );
        assert_eq!(
            collapse_repeats("られるから、頬へのキスだ。さすがに唇にするのは憚られるから、頬へのキスだ。"),
            "さすがに唇にするのは憚られるから、頬へのキスだ。"
        );
        // 短的首尾呼应是修辞，不动
        assert_eq!(collapse_repeats("ごめんね、本当にごめんね、"), "ごめんね、本当にごめんね、");
    }

    /// 模拟描边绘制：每个字符重复 n 遍。
    fn stroked(s: &str, n: usize) -> String {
        s.chars().flat_map(|c| std::iter::repeat(c).take(n)).collect()
    }

    #[test]
    fn collapses_doubled_characters() {
        assert_eq!(collapse_repeats("ここここんんににちちはは"), "ここんにちは");
        assert_eq!(collapse_repeats("ああいいうういい"), "あいうい");
    }

    /// 回归保护：句尾标点没跟着重复。
    ///
    /// 早先用 gcd 估 N，这一个长度为 1 的游程就把 gcd 拉回 1，整行原样放行 ——
    /// 而几乎每句正文都带标点，等于这条规则在真机上基本不生效。
    #[test]
    fn collapses_when_punctuation_is_not_repeated() {
        let raw = format!("{}。", stroked("窓の外はもう暗くなっていた", 3));
        assert_eq!(collapse_repeats(&raw), "窓の外はもう暗くなっていた。");

        let raw = format!("「{}」", stroked("まだ帰らないの？", 4));
        assert_eq!(collapse_repeats(&raw), "「まだ帰らないの？」");
    }

    /// 回归保护：最后一个字的描边只画了一半就被下一次钩子截断。
    #[test]
    fn collapses_with_truncated_tail() {
        let mut raw = stroked("彼女は静かにカップを置いた", 4);
        raw.push_str("。。"); // 只画了 2 遍
        assert_eq!(collapse_repeats(&raw), "彼女は静かにカップを置いた。");
    }

    /// 真实场景：整句被重复输出。单元长度远超任何自然叠词。
    #[test]
    fn collapses_whole_string_repetition() {
        assert_eq!(
            collapse_repeats("窓の外はもう暗くなっていた。窓の外はもう暗くなっていた。"),
            "窓の外はもう暗くなっていた。"
        );
        assert_eq!(
            collapse_repeats("少しだけ迷った。少しだけ迷った。少しだけ迷った。"),
            "少しだけ迷った。"
        );
    }

    /// 重复的一段后面还跟着别的内容 —— 钩子把上一句的尾巴和新句子拼在了一行里。
    #[test]
    fn collapses_repeated_head_before_new_content() {
        assert_eq!(
            collapse_repeats("そっと扉を閉めた。そっと扉を閉めた。それから振り返った。"),
            "そっと扉を閉めた。それから振り返った。"
        );
    }

    /// 两份之间夹着重发时补的空白。
    #[test]
    fn collapses_across_separating_whitespace() {
        assert_eq!(
            collapse_repeats(
                "少しだけ迷ってから、僕はうなずいた。 少しだけ迷ってから、僕はうなずいた。"
            ),
            "少しだけ迷ってから、僕はうなずいた。"
        );
    }

    /// 短单元一律不折 —— 宁可漏，不可误伤。
    #[test]
    fn short_units_are_never_collapsed() {
        for s in ["おはようおはよう", "またあしたまたあした", "はいはい"] {
            assert_eq!(collapse_repeats(s), s, "短单元不该被折叠：{s}");
        }
    }

    #[test]
    fn leaves_normal_prose_alone() {
        for s in [
            "窓の外はもう暗くなっていた。",
            "「まだ帰らないの？」",
            "そっと扉を閉めた。",
            "少しだけ迷ってから、僕はうなずいた。",
            "「ねえ、ちょっと待って。まだ話は終わってないんだけど」",
            "ドキドキが止まらない。そわそわしながら、そっと扉を開けた。",
            "うーん……そうだなあ、たぶん大丈夫だと思うけど。",
            "そうそう、それそれ。まさにそういうことだよ。",
            "はいはい、わかったわかった。今行くってば。",
            "彼は「おはよう」と言った。私も「おはよう」と返した。",
        ] {
            assert_eq!(collapse_repeats(s), s, "正常文本不该被改动：{s}");
        }
    }

    /// 这些在日文里是正常的，绝不能折叠。
    /// 二字重叠的拟声拟态词是最大的误伤风险来源。
    #[test]
    fn preserves_legitimate_repetition() {
        for s in [
            "ーーー",
            "ハハハ",
            "ああ",
            "そそる",
            "はは",
            "ドキドキ",
            "ワクワク",
            "キラキラ",
            "もじもじ",
            "そわそわ",
            "ドキドキドキドキ",
            "ーーーーーーーー",
            "……………………",
            "あああああ！　もうやだ！　ぜんぜんわからない！",
            "ぐーぐーぐーぐー、よく寝ているなあ。",
        ] {
            assert_eq!(collapse_repeats(s), s, "合法叠字被误伤：{s}");
        }
    }

    /// 连续的笑声/拟声段落：结构和描边重复同构，只能靠段数门槛挡住。
    #[test]
    fn preserves_runs_of_onomatopoeia() {
        for s in ["ははは、ふふふ、へへへ", "あははは、うふふふ、えへへへ"] {
            assert_eq!(collapse_repeats(s), s, "拟声段落被误伤：{s}");
        }
    }

    /// 真实形态：描边绘制 5 遍，每字出现 5 次；原文中本就重复的
    /// 字符游程长度是 2N=10（游程序列形如 [5, 10, 5, ...]）。
    #[test]
    fn collapses_n5_with_inherent_doubles() {
        // 原文「あいいうえ」，其中「いい」是原文自带的重复
        let raw: String = [('あ', 5), ('い', 10), ('う', 5), ('え', 5)]
            .iter()
            .flat_map(|&(ch, n)| std::iter::repeat(ch).take(n))
            .collect();
        assert_eq!(collapse_repeats(&raw), "あいいうえ");

        // 原文「ささやかな午後だった」，句首「ささ」游程为 10
        assert_eq!(
            collapse_repeats(&stroked("ささやかな午後だった", 5)),
            "ささやかな午後だった"
        );
    }

    /// 其他倍数：N=3、N=4 也要能折。
    #[test]
    fn collapses_other_multiplicities() {
        assert_eq!(collapse_repeats("あああいいいうううえええ"), "あいうえ");
        assert_eq!(collapse_repeats("かかかかききききくくくくけけけけ"), "かきくけ");
    }

    /// 游程段数不足时一律不折 —— 短字符串证据不足，误伤风险高。
    #[test]
    fn few_runs_are_never_collapsed() {
        for s in [
            "ああいい",                 // 2 段，且是可信的正常叠字形态
            "ああいいうう",             // 3 段，总长也不足
            "ああああいいいい",         // 2 段
            "ああああああああ",         // 1 段
            "あああいいいいいいううう", // 3 段
        ] {
            assert_eq!(collapse_repeats(s), s, "段数不足不该被折叠：{s}");
        }
    }

    #[test]
    fn handles_stacked_patterns() {
        // 先逐字加倍还原成「もじもじ」后就该停手 —— 那是个正常的词
        assert_eq!(collapse_repeats("ももじじももじじ"), "もじもじ");
        // 整句重复 + 逐字加倍同时存在
        let raw = stroked("窓の外はもう暗くなっていた。", 2).repeat(2);
        assert_eq!(collapse_repeats(&raw), "窓の外はもう暗くなっていた。");
    }

    #[test]
    fn is_a_noop_on_short_or_empty() {
        assert_eq!(collapse_repeats(""), "");
        assert_eq!(collapse_repeats("あ"), "あ");
        assert_eq!(collapse_repeats("です。"), "です。");
    }

    #[test]
    fn kana_ratio_separates_prose_from_ascii() {
        assert!(kana_ratio("こんにちは、世界") > 0.5);
        assert_eq!(kana_ratio("es.BT.NAME.SET"), 0.0);
    }
}
