pub mod lunahook;
pub mod replay;

use crate::model::HookLine;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// 一个文本来源：把 hook 行源源不断地送进 channel，直到 stop 被置位。
///
/// 抽出这一层是为了让开发和调试不依赖真实游戏 —— `replay` 用本地文件回放，
/// `lunahook` 才真正注入游戏。两者对上层完全等价。
pub trait Source: Send {
    fn run(self: Box<Self>, tx: Sender<HookLine>, stop: Arc<AtomicBool>) -> anyhow::Result<()>;
}
