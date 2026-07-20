//! `ai-agent-kit` — reusable AI agent runtime primitives.
//!
//! Wire types (inputs/outputs/config) live in `aiagentkit.v1` protobuf.
//! Runtime traits (`Llm`, `Tool`) adapt those messages to execution.
//!
//! Library code must not panic: failures are always returned as `Result` / errors.
//! The runtime is intended to build and run on Windows, macOS, and Linux
//! (Protobuf codegen via Bazel/`buf` remains Unix-oriented).

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]

mod agent;
mod fs_tools;
mod llm;
mod skills;
mod tools;

pub use agent::{format_skills_prompt, run_agent, Error};
pub use fs_tools::{register_workspace_fs_tools, MAX_READ_FILE_BYTES};
pub use llm::{AssistantTurn, Llm, LlmError, OpenAiCompatibleLlm, RetryingLlm};
pub use skills::{load_skills, Error as SkillsError};
pub use tools::{Tool, ToolError, ToolRegistry};

pub use api::aiagentkit::v1::{
    AgentTurn, DirEntry, DirListing, FileContent, ListDir, LoadSkills, Message,
    OpenAiCompatibleLlmConfig, ReadFile, RetryPolicy, Role, RunAgent, SkillBundle, SkillFile,
    ToolCall, ToolSpec,
};
/// Zero-copy buffa views (for decode-from-bytes paths).
pub use api::aiagentkit::v1::view::{
    AgentTurnOwnedView, AgentTurnView, DirEntryOwnedView, DirEntryView, DirListingOwnedView,
    DirListingView, FileContentOwnedView, FileContentView, ListDirOwnedView, ListDirView,
    LoadSkillsOwnedView, LoadSkillsView, MessageOwnedView, MessageView,
    OpenAiCompatibleLlmConfigOwnedView, OpenAiCompatibleLlmConfigView, ReadFileOwnedView,
    ReadFileView, RetryPolicyOwnedView, RetryPolicyView, RunAgentOwnedView, RunAgentView,
    SkillBundleOwnedView, SkillBundleView, SkillFileOwnedView, SkillFileView, ToolCallOwnedView,
    ToolCallView, ToolSpecOwnedView, ToolSpecView,
};
