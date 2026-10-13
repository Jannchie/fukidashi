use serde::Serialize;

/// 钩子送来的一句文本。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookLine {
    /// 唯一标识一路文本来源：pid:addr:ctx:ctx2。同一个 hook 函数被不同代码路径调用时
    /// 会分成不同的 key —— 这正是我们要挑的东西。
    pub key: String,
    /// hook 的名字，如 krkrz2 / GetGlyphOutlineW
    pub name: String,
    /// H-code，重新挂载同一个钩子时要用
    pub hcode: String,
    pub text: String,
}

/// 一路文本源的运行时统计，打分用。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadStat {
    pub key: String,
    pub name: String,
    pub hcode: String,
    /// 最近一条文本，界面上给用户预览
    pub sample: String,
    /// 收到过多少行
    pub count: usize,
    /// 去重后有多少行 —— 和 count 的比值反映这一路是不是在反复输出同样的东西
    pub unique_count: usize,
    /// 平均字符数
    pub avg_len: f32,
    /// 假名占比：正文几乎必然含大量平假名/片假名
    pub kana_ratio: f32,
    /// 综合得分 0~100
    pub score: f32,
    /// 是否疑似逐字输出（GetGlyphOutline 这类钩子的典型症状）
    pub per_glyph: bool,
}

/// 推给前端的一句话。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sentence {
    pub id: u64,
    pub key: String,
    pub text: String,
    /// 振假名分段；不含汉字或不像日文时为空
    pub ruby: Option<Vec<crate::furigana::RubySegment>>,
    /// 毫秒时间戳
    pub at: u64,
    /// 已交给翻译，译文随后按 id 推送
    pub translating: bool,
}

/// 前端订阅的事件负载。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// 新的一句正文（只有被选中的那一路才会产生）
    Sentence(Sentence),
    /// 线程统计刷新
    Threads { threads: Vec<ThreadStat>, active: Option<String>, pinned: Option<String>, warming_up: bool },
    /// 连接状态变化
    Status { connected: bool, detail: String },
    /// 某一句的译文（或失败原因），按 id 对上 Sentence。
    /// 流式输出时会先发若干次 done=false 的部分译文（每次都是到目前为止的全文）
    Translation { id: u64, text: Option<String>, error: Option<String>, done: bool },
    /// 连上游戏时先推一页这个游戏以前读过的句子
    Backlog { game: String, page: crate::history::Page },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    /// 可执行文件完整路径，用来给用户确认选对了游戏
    pub path: String,
    /// 可见主窗口的标题；有它就说明是前台应用，游戏基本都在这一类
    pub title: Option<String>,
    /// exe 版本资源里的 FileDescription，比文件名更好认
    pub description: Option<String>,
    /// exe 图标，PNG data URL；只给有窗口的进程取，省时间
    pub icon: Option<String>,
    /// exe 是否 64 位；只给有窗口的进程查
    pub is64: Option<bool>,
}
