//! `ai-agent-kit` — reusable AI agent runtime primitives.
//!
//! Wire types (inputs/outputs/config) live in `aiagentkit.v1` protobuf.
//! Runtime traits (`Llm`, `Tool`) adapt those messages to execution.

mod agent;
mod llm;
mod skills;
mod tools;

pub use agent::{format_skills_prompt, run_agent, Error};
pub use llm::{AssistantTurn, Llm, LlmError, OpenAiCompatibleLlm, RetryingLlm};
pub use skills::{load_skills, Error as SkillsError};
pub use tools::{Tool, ToolError, ToolRegistry};

pub use api::aiagentkit::v1::{
    AgentTurn, LoadSkills, Message, OpenAiCompatibleLlmConfig, RetryPolicy, Role, RunAgent,
    SkillBundle, SkillFile, ToolCall, ToolSpec,
};
/// Zero-copy buffa views (for decode-from-bytes paths).
pub use api::aiagentkit::v1::view::{
    AgentTurnOwnedView, AgentTurnView, LoadSkillsOwnedView, LoadSkillsView, MessageOwnedView,
    MessageView, OpenAiCompatibleLlmConfigOwnedView, OpenAiCompatibleLlmConfigView,
    RetryPolicyOwnedView, RetryPolicyView, RunAgentOwnedView, RunAgentView, SkillBundleOwnedView,
    SkillBundleView, SkillFileOwnedView, SkillFileView, ToolCallOwnedView, ToolCallView,
    ToolSpecOwnedView, ToolSpecView,
};
