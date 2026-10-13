//! 翻译：对接任意 OpenAI 兼容接口（Ollama / LM Studio / llama.cpp server / 云端 API）。
//!
//! 刻意不在 Fukidashi 里加载模型：模型的运行、量化、更新都交给这些成熟的服务，
//! Fukidashi 只管挑模型、带上下文发请求。

use crate::model::Event;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// 快进刷屏时只翻最新这么多句，避免队列越积越长、译文越来越滞后。
const MAX_PENDING: usize = 3;
/// 本地模型首次加载可能要十几秒，留足余量。
const TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslatorConfig {
    pub enabled: bool,
    /// 形如 http://127.0.0.1:11434/v1
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    /// 目标语言，直接写进提示词，如「简体中文」
    pub target: String,
    /// 带上前几句的原文与译文作上下文，保持人名、语气一致
    pub context: usize,
}

impl TranslatorConfig {
    /// 开着且选了模型
    fn ready(&self) -> bool {
        self.enabled && !self.model.is_empty()
    }
}

pub struct Translator {
    config: Arc<Mutex<Option<TranslatorConfig>>>,
    queue: UnboundedSender<Job>,
}

enum Job {
    Line { id: u64, text: String },
    /// 换了游戏或重新附加：上下文作废
    Reset,
}

impl Translator {
    /// `on_done` 在每句译完时调用（id, 译文），用来落库
    pub fn new(app: AppHandle, on_done: impl Fn(u64, &str) + Send + 'static) -> Self {
        let (tx, rx) = unbounded_channel();
        let config = Arc::new(Mutex::new(None));
        tauri::async_runtime::spawn(worker(app, config.clone(), rx, on_done));
        Self { config, queue: tx }
    }

    pub fn configure(&self, cfg: TranslatorConfig) {
        *self.config.lock().unwrap() = Some(cfg);
    }

    /// 交给翻译；翻译没开时返回 false
    pub fn submit(&self, id: u64, text: &str) -> bool {
        let ready = self.config.lock().unwrap().as_ref().is_some_and(TranslatorConfig::ready);
        if ready {
            let _ = self.queue.send(Job::Line { id, text: text.to_string() });
        }
        ready
    }

    pub fn reset(&self) {
        let _ = self.queue.send(Job::Reset);
    }
}

async fn worker(
    app: AppHandle,
    config: Arc<Mutex<Option<TranslatorConfig>>>,
    mut rx: UnboundedReceiver<Job>,
    on_done: impl Fn(u64, &str),
) {
    let client = reqwest::Client::builder().timeout(TIMEOUT).build().expect("HTTP 客户端");
    let mut history: VecDeque<(String, String)> = VecDeque::new();
    let mut pending: VecDeque<(u64, String)> = VecDeque::new();

    loop {
        // 一次把积压的都取出来，再决定翻哪些
        if pending.is_empty() {
            match rx.recv().await {
                Some(job) => apply(job, &mut pending, &mut history),
                None => return,
            }
        }
        while let Ok(job) = rx.try_recv() {
            apply(job, &mut pending, &mut history);
        }
        while pending.len() > MAX_PENDING {
            let (id, _) = pending.pop_front().unwrap();
            emit(&app, id, None, Some("跳过：句子来得太快".into()), true);
        }
        let Some((id, text)) = pending.pop_front() else { continue };

        let Some(cfg) = config.lock().unwrap().clone().filter(TranslatorConfig::ready) else { continue };
        let skip = history.len().saturating_sub(cfg.context);
        let ctx: Vec<_> = history.iter().skip(skip).cloned().collect();

        let partial = |so_far: &str| emit(&app, id, Some(so_far.to_string()), None, false);
        match translate(&client, &cfg, &ctx, &text, partial).await {
            Ok(out) => {
                history.push_back((text, out.clone()));
                while history.len() > 32 {
                    history.pop_front();
                }
                on_done(id, &out);
                emit(&app, id, Some(out), None, true);
            }
            Err(e) => emit(&app, id, None, Some(e), true),
        }
    }
}

fn apply(job: Job, pending: &mut VecDeque<(u64, String)>, history: &mut VecDeque<(String, String)>) {
    match job {
        Job::Line { id, text } => pending.push_back((id, text)),
        Job::Reset => {
            pending.clear();
            history.clear();
        }
    }
}

fn emit(app: &AppHandle, id: u64, text: Option<String>, error: Option<String>, done: bool) {
    let _ = app.emit(crate::EVENT, Event::Translation { id, text, error, done });
}

/// 专门训练过的翻译模型各有固定的提示词格式，不按它来质量会明显下降。
/// 按模型名识别；其余模型用通用的指令式提示词。
#[derive(Debug, PartialEq)]
enum Template {
    /// Sakura-GalTransl / GalTransl，以及沿用同一格式的 Sakura v1.5（Qwen3）：
    /// 历史译文拼进同一条用户消息
    GalTransl { v15: bool },
    /// Sakura 系列：多轮对话，用户消息带固定前缀
    Sakura,
    Generic,
}

impl Template {
    fn of(model: &str) -> Self {
        let m = model.to_lowercase();
        if m.contains("galtransl") {
            Self::GalTransl { v15: false }
        } else if m.contains("sakura") && (m.contains("qwen3") || m.contains("v1.5")) {
            Self::GalTransl { v15: true }
        } else if m.contains("sakura") {
            Self::Sakura
        } else {
            Self::Generic
        }
    }

    /// Sakura / GalTransl 只训练过日译简中，提示词也写死了简体中文；
    /// 目标语言是别的时候改用通用提示词，不然配置里的目标语言会被悄悄忽略
    fn for_config(cfg: &TranslatorConfig) -> Self {
        match Self::of(&cfg.model) {
            Self::GalTransl { .. } | Self::Sakura if cfg.target != "简体中文" => Self::Generic,
            t => t,
        }
    }

    /// (temperature, top_p)，取各模型作者的推荐值
    fn sampling(&self) -> (f32, f32) {
        match self {
            Self::GalTransl { .. } => (0.3, 0.8),
            Self::Sakura => (0.1, 0.3),
            Self::Generic => (0.2, 0.9),
        }
    }
}

fn messages(cfg: &TranslatorConfig, ctx: &[(String, String)], text: &str) -> Vec<Value> {
    let system = |s: &str| json!({ "role": "system", "content": s });
    let user = |s: String| json!({ "role": "user", "content": s });

    match Template::for_config(cfg) {
        Template::GalTransl { v15 } => {
            // 与 LunaTranslator 一致：术语表为空时整段省略。按模型卡留一个空术语表段落，
            // 模型会把历史和指令当成原文一起复述出来。
            let mut prompt = String::new();
            if !ctx.is_empty() {
                let history: Vec<&str> = ctx.iter().map(|(_, dst)| dst.as_str()).collect();
                prompt += &format!("历史翻译：{}

结合历史剧情和上下文，", history.join("
"));
            }
            prompt += "将下面的文本从日文翻译成简体中文：
";
            prompt += text;
            let sys = if v15 {
                "你是一个日本二次元领域的日语翻译模型，可以流畅通顺地以日本轻小说/漫画/Galgame的风格将日文翻译成简体中文，                 并联系上下文正确使用人称代词，不擅自添加原文中没有的代词。"
            } else {
                "你是一个视觉小说翻译模型，可以通顺地使用给定的术语表以指定的风格将日文翻译成简体中文，                 并联系上下文正确使用人称代词，注意不要混淆使役态和被动态的主语和宾语，                 不要擅自添加原文中没有的特殊符号，也不要擅自增加或减少换行。"
            };
            vec![system(sys), user(prompt)]
        }
        Template::Sakura => {
            let ask = |t: &str| format!("将下面的日文文本翻译成中文：{t}");
            let mut msgs = vec![system(
                "你是一个轻小说翻译模型，可以流畅通顺地以日本轻小说的风格将日文翻译成简体中文，                 并联系上下文正确使用人称代词，不擅自添加原文中没有的代词。",
            )];
            for (src, dst) in ctx {
                msgs.push(user(ask(src)));
                msgs.push(json!({ "role": "assistant", "content": dst }));
            }
            msgs.push(user(ask(text)));
            msgs
        }
        Template::Generic => {
            let mut msgs = vec![system(&format!(
                "你是视觉小说的翻译。把用户给出的日文台词翻译成{}。                 保持人名、称呼与前文一致，保留说话人的语气，「」等引号原样保留。                 只输出译文本身，不要解释、不要注音、不要加引号以外的任何内容。",
                cfg.target
            ))];
            for (src, dst) in ctx {
                msgs.push(user(src.clone()));
                msgs.push(json!({ "role": "assistant", "content": dst }));
            }
            msgs.push(user(text.to_string()));
            msgs
        }
    }
}

fn endpoint(base: &str, path: &str) -> String {
    format!("{}/{path}", base.trim().trim_end_matches('/'))
}

fn authed(req: reqwest::RequestBuilder, key: &str) -> reqwest::RequestBuilder {
    if key.trim().is_empty() { req } else { req.bearer_auth(key.trim()) }
}

async fn translate(
    client: &reqwest::Client,
    cfg: &TranslatorConfig,
    ctx: &[(String, String)],
    text: &str,
    on_partial: impl Fn(&str),
) -> Result<String, String> {
    let (temperature, top_p) = Template::for_config(cfg).sampling();
    let body = json!({
        "model": cfg.model,
        "messages": messages(cfg, ctx, text),
        "temperature": temperature,
        "top_p": top_p,
        "stream": true,
    });
    let mut resp = authed(client.post(endpoint(&cfg.base_url, "chat/completions")), &cfg.api_key)
        .json(&body)
        .send()
        .await
        .map_err(describe)?;
    let status = resp.status();
    if !status.is_success() {
        let v: Value = resp.json().await.unwrap_or_default();
        let msg = v.pointer("/error/message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("HTTP {status} {msg}"));
    }

    // 服务端推送（SSE）：一行一个 `data: {...}`，以 `data: [DONE]` 结束。
    // 个别服务无视 stream 直接回整段 JSON，也一并兼容。
    let mut buf: Vec<u8> = Vec::new();
    let mut out = String::new();
    let mut shown = String::new();
    while let Some(chunk) = resp.chunk().await.map_err(describe)? {
        buf.extend_from_slice(&chunk);
        while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=nl).collect();
            let line = String::from_utf8_lossy(&line);
            if let Some(delta) = sse_delta(line.trim()) {
                out += &delta;
                let now = clean(&out, false);
                if now != shown && !now.is_empty() {
                    shown = now.to_string();
                    on_partial(&shown);
                }
            }
        }
    }
    if out.is_empty() {
        // 不是 SSE：按普通响应解析剩下的整段
        let v: Value = serde_json::from_slice(&buf).map_err(|e| format!("响应不是 JSON：{e}"))?;
        out = v
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or("响应里没有译文")?
            .to_string();
    }
    Ok(clean(&out, true).to_string())
}

/// 从一行 SSE 里取出增量文本
fn sse_delta(line: &str) -> Option<String> {
    let data = line.strip_prefix("data:")?.trim();
    if data == "[DONE]" {
        return None;
    }
    let v: Value = serde_json::from_str(data).ok()?;
    v.pointer("/choices/0/delta/content").and_then(Value::as_str).map(String::from)
}

/// 去掉思考过程和复述的提示词。`done` 为假时是半截输出：还在 <think> 里就什么都不显示。
fn clean(s: &str, done: bool) -> &str {
    if !done && s.trim_start().starts_with("<think>") && !s.contains("</think>") {
        return "";
    }
    strip_echo(strip_think(s)).trim()
}

/// 小模型偶尔会把历史和指令一起复述出来。指令句只会出现在提示词里，
/// 一旦在输出中看到它，真正的译文只可能在它后面。
fn strip_echo(s: &str) -> &str {
    const MARK: &str = "翻译成简体中文：";
    match s.rfind(MARK) {
        Some(i) => &s[i + MARK.len()..],
        None => s,
    }
}

/// 推理模型（Qwen3、DeepSeek-R1 等）会把思考过程包在 <think> 里一并返回。
fn strip_think(s: &str) -> &str {
    match s.find("</think>") {
        Some(i) => &s[i + "</think>".len()..],
        None => s,
    }
}

fn describe(e: reqwest::Error) -> String {
    if e.is_connect() {
        "连不上翻译服务，确认它已启动、地址正确".into()
    } else if e.is_timeout() {
        "翻译超时".into()
    } else {
        e.to_string()
    }
}

/// 列出服务上可用的模型。
pub async fn list_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = authed(client.get(endpoint(base_url, "models")), api_key)
        .send()
        .await
        .map_err(describe)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    let v: Value = resp.json().await.map_err(|e| format!("响应不是 JSON：{e}"))?;
    let mut ids: Vec<String> = v["data"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    ids.sort();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(model: &str) -> TranslatorConfig {
        TranslatorConfig {
            enabled: true,
            base_url: "http://x/v1/".into(),
            model: model.into(),
            api_key: String::new(),
            target: "简体中文".into(),
            context: 2,
        }
    }

    #[test]
    fn parses_sse_lines() {
        assert_eq!(
            sse_delta(r#"data: {"choices":[{"delta":{"content":"你好"}}]}"#).as_deref(),
            Some("你好")
        );
        assert_eq!(sse_delta("data: [DONE]"), None);
        assert_eq!(sse_delta(": keep-alive"), None);
    }

    #[test]
    fn hides_unfinished_reasoning() {
        assert_eq!(clean("<think>嗯", false), "");
        assert_eq!(clean("<think>嗯</think>\n你", false), "你");
    }

    #[test]
    fn strips_reasoning() {
        assert_eq!(strip_think("<think>嗯……</think>\n你好"), "\n你好");
        assert_eq!(strip_think("你好"), "你好");
    }

    #[test]
    fn joins_endpoint_without_double_slash() {
        assert_eq!(endpoint("http://x/v1/", "models"), "http://x/v1/models");
    }

    #[test]
    fn context_becomes_alternating_turns() {
        let ctx = vec![("おはよう".into(), "早上好".into())];
        let m = messages(&cfg("qwen3:14b"), &ctx, "こんにちは");
        let roles: Vec<_> = m.iter().map(|x| x["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["system", "user", "assistant", "user"]);
        assert!(m[0]["content"].as_str().unwrap().contains("简体中文"));
    }

    #[test]
    fn galtransl_puts_history_into_one_user_message() {
        let ctx = vec![("おはよう".into(), "早上好".into()), ("うん".into(), "嗯".into())];
        let m = messages(&cfg("sakura-galtransl-14b-v3.8"), &ctx, "こんにちは");
        assert_eq!(m.len(), 2);
        let u = m[1]["content"].as_str().unwrap();
        assert_eq!(u, "历史翻译：早上好
嗯

结合历史剧情和上下文，将下面的文本从日文翻译成简体中文：
こんにちは");
    }

    #[test]
    fn galtransl_without_history_is_just_the_instruction() {
        let m = messages(&cfg("galtransl-v4-4b"), &[], "こんにちは");
        assert_eq!(m[1]["content"], "将下面的文本从日文翻译成简体中文：
こんにちは");
    }

    #[test]
    fn other_targets_fall_back_to_generic_prompt() {
        let mut c = cfg("sakura-galtransl-14b-v3.8");
        c.target = "English".into();
        assert_eq!(Template::for_config(&c), Template::Generic);
        assert!(messages(&c, &[], "こんにちは")[0]["content"].as_str().unwrap().contains("English"));
    }

    #[test]
    fn sakura_v15_uses_galtransl_layout() {
        assert_eq!(Template::of("sakura-14b-qwen3-v1.5"), Template::GalTransl { v15: true });
        assert_eq!(Template::of("sakura-14b-qwen2.5-v1.0"), Template::Sakura);
    }

    #[test]
    fn strips_echoed_prompt() {
        let echoed = "千明「那可不行」 参考以下术语表： 根据以上术语表，将下面的文本从日文翻译成简体中文： 莲「啊……」";
        assert_eq!(strip_echo(echoed).trim(), "莲「啊……」");
        assert_eq!(strip_echo("莲「啊……」"), "莲「啊……」");
    }

    #[test]
    fn sakura_uses_its_own_template() {
        let m = messages(&cfg("sakura-14b-qwen2.5-v1.0"), &[], "こんにちは");
        assert_eq!(m[1]["content"], "将下面的日文文本翻译成中文：こんにちは");
    }
}
