//! Smoke test against NVIDIA NIM (GLM-5.2).
//!
//! ```bash
//! export NVIDIA_API_KEY=...
//! export NVIDIA_BASE_URL=https://integrate.api.nvidia.com/v1   # optional
//! export NVIDIA_MODEL=z-ai/glm-5.2                            # optional
//! cargo run -p ai-agent-kit --example nvidia_smoke
//! ```

use std::env;

use ai_agent_kit::{
    run_agent, OpenAiCompatibleLlm, OpenAiCompatibleLlmConfig, RetryPolicy, RetryingLlm, RunAgent,
    ToolRegistry,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("NVIDIA_API_KEY")
        .map_err(|_| "set NVIDIA_API_KEY")?
        .trim()
        .to_owned();
    if api_key.is_empty() {
        return Err("NVIDIA_API_KEY is empty".into());
    }

    let base_url = env::var("NVIDIA_BASE_URL")
        .unwrap_or_else(|_| "https://integrate.api.nvidia.com/v1".to_owned())
        .trim()
        .trim_end_matches('/')
        .to_owned();
    let model = env::var("NVIDIA_MODEL").unwrap_or_else(|_| "z-ai/glm-5.2".to_owned());

    let llm_config = OpenAiCompatibleLlmConfig::default()
        .with_base_url(base_url)
        .with_api_key(api_key)
        .with_model(model);
    let retry = RetryPolicy::default()
        .with_max_attempts(3)
        .with_initial_backoff_ms(200)
        .with_max_backoff_ms(5000);

    let llm = RetryingLlm::new(OpenAiCompatibleLlm::new(llm_config)?, retry)?;
    let tools = ToolRegistry::new();

    let params = RunAgent::default()
        .with_system_prompt("Reply in one short sentence.")
        .with_user_message("Say hello and name the model family you belong to.")
        .with_max_steps(1);

    let turn = run_agent(&params, &llm, &tools).await?;
    println!("{}", turn.final_text.as_deref().unwrap_or("<empty>"));
    println!("(steps={})", turn.steps.unwrap_or(0));
    Ok(())
}
