//! `ai-agent` — thin CLI over [`ai_agent_kit`].
//!
//! Builds protobuf configs from flags/env, registers workspace FS tools, runs the agent.
//!
//! The API key itself is never a CLI flag (Cargo echoes argv). Pass the *name* of an
//! env var via `--api-key-env-var`.
//!
//! ```text
//! cargo run -p ai-agent -- \
//!   --base-url https://integrate.api.nvidia.com/v1 \
//!   --api-key-env-var NVIDIA_API_KEY \
//!   --model z-ai/glm-5.2 \
//!   --workspace . \
//!   --message "List files in the repo root"
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use ai_agent_kit::{
    register_workspace_fs_tools, run_agent, run_agent_with_observer, OpenAiCompatibleLlm,
    OpenAiCompatibleLlmConfig, RetryPolicy, RetryingLlm, RunAgent, StderrObserver, ToolRegistry,
};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "ai-agent",
    about = "Run an ai-agent-kit turn (OpenAI-compatible LLM + workspace tools)"
)]
struct Args {
    /// Workspace root (skills under `.skills/`, FS tools sandboxed here).
    #[arg(long, short = 'w', default_value = ".", env = "AI_AGENT_WORKSPACE")]
    workspace: PathBuf,

    /// Optional skill id under `.skills/<id>/`.
    #[arg(long, short = 's', env = "AI_AGENT_SKILL")]
    skill: Option<String>,

    /// Extra system prompt (appended after skill markdown when both set).
    #[arg(long, env = "AI_AGENT_SYSTEM_PROMPT")]
    system_prompt: Option<String>,

    /// User message that starts the agent turn.
    #[arg(long, short = 'm', env = "AI_AGENT_MESSAGE")]
    message: String,

    /// Max LLM ↔ tool iterations.
    #[arg(long, default_value_t = 8, env = "AI_AGENT_MAX_STEPS")]
    max_steps: u32,

    /// OpenAI-compatible API base URL (including `/v1`).
    #[arg(long, env = "AI_AGENT_BASE_URL")]
    base_url: String,

    /// Name of the env var that holds the Bearer API key (not the key value).
    #[arg(long, default_value = "AI_AGENT_API_KEY", env = "AI_AGENT_API_KEY_ENV_VAR")]
    api_key_env_var: String,

    /// Model id.
    #[arg(long, env = "AI_AGENT_MODEL")]
    model: String,

    /// Total LLM HTTP attempts including the first.
    #[arg(long, default_value_t = 3, env = "AI_AGENT_RETRY_MAX_ATTEMPTS")]
    retry_max_attempts: u32,

    /// Initial retry backoff in milliseconds.
    #[arg(long, default_value_t = 200, env = "AI_AGENT_RETRY_INITIAL_BACKOFF_MS")]
    retry_initial_backoff_ms: u32,

    /// Max retry backoff in milliseconds.
    #[arg(long, default_value_t = 5000, env = "AI_AGENT_RETRY_MAX_BACKOFF_MS")]
    retry_max_backoff_ms: u32,

    /// Do not register `read_file` / `list_dir` / `write_file`.
    #[arg(long, default_value_t = false)]
    no_fs_tools: bool,

    /// Log LLM steps and tool calls/results to stderr (truncated).
    #[arg(long, short = 'v', default_value_t = false, env = "AI_AGENT_VERBOSE")]
    verbose: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Read the API key from the named env var (never from argv).
fn resolve_api_key(env_var: &str) -> Result<String, String> {
    let name = env_var.trim();
    if name.is_empty() {
        return Err("api-key-env-var must not be empty".into());
    }
    if name.contains('=') || name.contains('\0') || name.contains('/') || name.contains('\\') {
        return Err(format!("invalid api-key-env-var name: {name:?}"));
    }

    match std::env::var(name) {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err(format!("environment variable {name} is set but empty"))
            } else {
                Ok(trimmed.to_owned())
            }
        }
        Err(std::env::VarError::NotPresent) => {
            Err(format!("environment variable {name} is not set"))
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("environment variable {name} is not valid UTF-8"))
        }
    }
}

#[tokio::main]
async fn run() -> Result<(), String> {
    let args = Args::parse();

    if args.message.trim().is_empty() {
        return Err("message must not be empty".into());
    }
    if args.max_steps < 1 {
        return Err("max-steps must be >= 1".into());
    }
    if args.base_url.trim().is_empty() {
        return Err("base-url must not be empty (flag or AI_AGENT_BASE_URL)".into());
    }
    if args.model.trim().is_empty() {
        return Err("model must not be empty (flag or AI_AGENT_MODEL)".into());
    }
    if args.retry_max_attempts < 1 {
        return Err("retry-max-attempts must be >= 1".into());
    }
    if args.retry_max_backoff_ms < 1 {
        return Err("retry-max-backoff-ms must be >= 1".into());
    }

    let api_key = resolve_api_key(&args.api_key_env_var)?;

    let workspace = args
        .workspace
        .canonicalize()
        .map_err(|e| format!("workspace {}: {e}", args.workspace.display()))?;

    let llm_config = OpenAiCompatibleLlmConfig::default()
        .with_base_url(args.base_url.trim().trim_end_matches('/'))
        .with_api_key(api_key)
        .with_model(args.model.trim());

    let retry = RetryPolicy::default()
        .with_max_attempts(args.retry_max_attempts)
        .with_initial_backoff_ms(args.retry_initial_backoff_ms)
        .with_max_backoff_ms(args.retry_max_backoff_ms);

    let llm = OpenAiCompatibleLlm::new(llm_config).map_err(|e| e.to_string())?;
    let llm = RetryingLlm::new(llm, retry).map_err(|e| e.to_string())?;

    let mut tools = ToolRegistry::new();
    if !args.no_fs_tools {
        register_workspace_fs_tools(&mut tools, &workspace);
    }

    let mut params = RunAgent::default()
        .with_workspace_root(workspace.to_string_lossy())
        .with_user_message(args.message.trim())
        .with_max_steps(args.max_steps);

    if let Some(skill) = args.skill.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        params = params.with_skill_id(skill);
    }
    if let Some(system) = args
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        params = params.with_system_prompt(system);
    }

    let turn = if args.verbose {
        run_agent_with_observer(&params, &llm, &tools, &StderrObserver::default())
            .await
            .map_err(|e| e.to_string())?
    } else {
        run_agent(&params, &llm, &tools)
            .await
            .map_err(|e| e.to_string())?
    };

    let text = turn.final_text.as_deref().unwrap_or("");
    println!("{text}");
    eprintln!("(steps={})", turn.steps.unwrap_or(0));
    Ok(())
}
