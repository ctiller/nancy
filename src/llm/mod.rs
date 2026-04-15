// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub mod api;
pub mod builder;
pub mod client;
pub mod mock;
pub mod schema;
pub mod tool;

pub use builder::{LlmBuilder, fast_llm, lite_llm, thinking_llm};
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
