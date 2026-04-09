pub mod builder;
pub mod client;
pub mod mock;
pub mod schema;
pub mod tool;

pub use builder::{LlmBuilder, fast_llm, thinking_llm};
pub use client::LlmClient;
pub use llm_macros::{llm_tool, make_tool};
pub use tool::LlmTool;

use std::sync::atomic::{AtomicBool, Ordering};

static LLM_BANNED: AtomicBool = AtomicBool::new(false);

/// Permanently bans LLM instantiation in the current process.
pub fn ban_llm() {
    LLM_BANNED.store(true, Ordering::SeqCst);
}

pub fn unban_llm() {
    LLM_BANNED.store(false, Ordering::SeqCst);
}

pub fn is_llm_banned() -> bool {
    LLM_BANNED.load(Ordering::SeqCst)
}
