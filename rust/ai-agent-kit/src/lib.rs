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
mod observe;
mod shell_tools;
mod skills;
mod tools;

pub use agent::{format_skills_prompt, run_agent, run_agent_with_observer, Error};
pub use fs_tools::{
    register_workspace_fs_tools, MAX_READ_FILE_BYTES, MAX_WRITE_FILE_BYTES,
};
pub use llm::{AssistantTurn, Llm, LlmError, OpenAiCompatibleLlm, RetryingLlm};
pub use observe::{AgentObserver, NoopObserver, StderrObserver};
pub use shell_tools::register_workspace_shell_tool;
pub use skills::{load_skills, Error as SkillsError};
pub use tools::{Tool, ToolError, ToolRegistry};

pub use api::aiagentkit::v1::{
    AgentLlmStep, AgentToolCall, AgentToolResult, AgentTurn, CommandResult, DirEntry, DirListing,
    FileContent, ListDir, LoadSkills, Message, OpenAiCompatibleLlmConfig, ReadFile, RetryPolicy,
    Role, RunAgent, RunCommand, ShellToolConfig, SkillBundle, SkillFile, ToolCall, ToolSpec,
    WriteFile, WriteFileResult,
};
/// Zero-copy buffa views (for decode-from-bytes paths).
pub use api::aiagentkit::v1::view::{
    AgentLlmStepOwnedView, AgentLlmStepView, AgentToolCallOwnedView, AgentToolCallView,
    AgentToolResultOwnedView, AgentToolResultView, AgentTurnOwnedView, AgentTurnView,
    CommandResultOwnedView, CommandResultView, DirEntryOwnedView, DirEntryView, DirListingOwnedView,
    DirListingView, FileContentOwnedView, FileContentView, ListDirOwnedView, ListDirView,
    LoadSkillsOwnedView, LoadSkillsView, MessageOwnedView, MessageView,
    OpenAiCompatibleLlmConfigOwnedView, OpenAiCompatibleLlmConfigView, ReadFileOwnedView,
    ReadFileView, RetryPolicyOwnedView, RetryPolicyView, RunAgentOwnedView, RunAgentView,
    RunCommandOwnedView, RunCommandView, ShellToolConfigOwnedView, ShellToolConfigView,
    SkillBundleOwnedView, SkillBundleView, SkillFileOwnedView, SkillFileView, ToolCallOwnedView,
    ToolCallView, ToolSpecOwnedView, ToolSpecView, WriteFileOwnedView, WriteFileResultOwnedView,
    WriteFileResultView, WriteFileView,
};
