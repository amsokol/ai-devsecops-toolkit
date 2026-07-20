//! LLM trait and OpenAI-compatible HTTP client.
//!
//! Endpoint / model / key / retry policy come from protobuf config messages.

use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use api::aiagentkit::v1::{
    Message, OpenAiCompatibleLlmConfig, RetryPolicy, Role, ToolCall, ToolSpec,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One assistant completion: text and/or tool calls (maps to an assistant [`Message`]).
#[derive(Debug, Clone, Default)]
pub struct AssistantTurn {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

/// Errors from an [`Llm`] implementation.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("llm config: {0}")]
    InvalidConfig(&'static str),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API {status}: {body}")]
    Api { status: u16, body: String },

    #[error("empty choices in chat completion response")]
    EmptyChoices,

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("invalid tool parameters_json: {0}")]
    InvalidToolParameters(String),
}

impl LlmError {
    /// Whether a caller may safely retry the same completion request.
    ///
    /// Retriable: timeouts / connect failures, 408/429/5xx, empty choices.
    /// Not retriable: auth, bad request, parse / config errors.
    pub fn is_retriable(&self) -> bool {
        match self {
            Self::InvalidConfig(_) | Self::InvalidToolParameters(_) | Self::InvalidResponse(_) => {
                false
            }
            Self::Http(err) => {
                if err.is_timeout() || err.is_connect() {
                    return true;
                }
                err.status()
                    .map(|s| is_retriable_status(s.as_u16()))
                    .unwrap_or(false)
            }
            Self::Api { status, .. } => is_retriable_status(*status),
            Self::EmptyChoices => true,
        }
    }
}

fn is_retriable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

/// Chat-completions style LLM used by the agent loop.
pub trait Llm: Send + Sync {
    fn complete<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [ToolSpec],
    ) -> Pin<Box<dyn Future<Output = Result<AssistantTurn, LlmError>> + Send + 'a>>;
}

/// Wraps an [`Llm`] and retries [`LlmError::is_retriable`] failures per [`RetryPolicy`].
pub struct RetryingLlm<L> {
    inner: L,
    policy: RetryPolicy,
}

impl<L> RetryingLlm<L> {
    pub fn new(inner: L, policy: RetryPolicy) -> Result<Self, LlmError> {
        validate_retry_policy(&policy)?;
        Ok(Self { inner, policy })
    }
}

fn validate_retry_policy(policy: &RetryPolicy) -> Result<(), LlmError> {
    let max_attempts = policy.max_attempts.unwrap_or(0);
    if max_attempts < 1 {
        return Err(LlmError::InvalidConfig("retry max_attempts must be >= 1"));
    }
    if policy.max_backoff_ms.unwrap_or(0) < 1 {
        return Err(LlmError::InvalidConfig("retry max_backoff_ms must be >= 1"));
    }
    Ok(())
}

impl<L: Llm> Llm for RetryingLlm<L> {
    fn complete<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [ToolSpec],
    ) -> Pin<Box<dyn Future<Output = Result<AssistantTurn, LlmError>> + Send + 'a>> {
        Box::pin(async move {
            let max = self.policy.max_attempts.unwrap_or(1).max(1);
            let initial_ms = self.policy.initial_backoff_ms.unwrap_or(0);
            let max_ms = self.policy.max_backoff_ms.unwrap_or(1).max(1);
            let mut backoff = Duration::from_millis(u64::from(initial_ms));
            let max_backoff = Duration::from_millis(u64::from(max_ms));
            let mut attempt = 0u32;

            loop {
                attempt += 1;
                match self.inner.complete(messages, tools).await {
                    Ok(turn) => return Ok(turn),
                    Err(err) if err.is_retriable() && attempt < max => {
                        tokio::time::sleep(jittered(backoff)).await;
                        backoff = backoff
                            .checked_mul(2)
                            .unwrap_or(max_backoff)
                            .min(max_backoff);
                    }
                    Err(err) => return Err(err),
                }
            }
        })
    }
}

/// Full-jitter delay in `[backoff/2, backoff]`.
fn jittered(backoff: Duration) -> Duration {
    let millis = backoff.as_millis() as u64;
    if millis <= 1 {
        return backoff;
    }
    let half = millis / 2;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let span = millis - half;
    Duration::from_millis(half + (nanos % (span + 1)))
}

/// OpenAI Chat Completions client (`POST {base}/chat/completions`).
pub struct OpenAiCompatibleLlm {
    client: reqwest::Client,
    config: OpenAiCompatibleLlmConfig,
}

impl OpenAiCompatibleLlm {
    pub fn new(config: OpenAiCompatibleLlmConfig) -> Result<Self, LlmError> {
        validate_llm_config(&config)?;
        Ok(Self {
            client: reqwest::Client::new(),
            config,
        })
    }
}

fn validate_llm_config(config: &OpenAiCompatibleLlmConfig) -> Result<(), LlmError> {
    if config
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err(LlmError::InvalidConfig("base_url is required"));
    }
    if config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err(LlmError::InvalidConfig("api_key is required"));
    }
    if config
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        return Err(LlmError::InvalidConfig("model is required"));
    }
    Ok(())
}

impl Llm for OpenAiCompatibleLlm {
    fn complete<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [ToolSpec],
    ) -> Pin<Box<dyn Future<Output = Result<AssistantTurn, LlmError>> + Send + 'a>> {
        Box::pin(async move { self.complete_inner(messages, tools).await })
    }
}

impl OpenAiCompatibleLlm {
    async fn complete_inner(
        &self,
        messages: &[Message],
        tools: &[ToolSpec],
    ) -> Result<AssistantTurn, LlmError> {
        let base = self
            .config
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or(LlmError::InvalidConfig("base_url is required"))?
            .trim_end_matches('/');
        let url = format!("{base}/chat/completions");
        let model = self
            .config
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or(LlmError::InvalidConfig("model is required"))?;
        let api_key = self
            .config
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or(LlmError::InvalidConfig("api_key is required"))?;

        let mut body = json!({
            "model": model,
            "messages": messages.iter().map(to_openai_message).collect::<Vec<_>>(),
        });

        if !tools.is_empty() {
            let openai_tools: Result<Vec<_>, _> = tools.iter().map(to_openai_tool).collect();
            body["tools"] = Value::Array(openai_tools?);
            body["tool_choice"] = json!("auto");
        }

        let response = self
            .client
            .post(&url)
            .bearer_auth(api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(LlmError::Api {
                status: status.as_u16(),
                body: text,
            });
        }

        let parsed: ChatCompletionResponse = serde_json::from_str(&text)
            .map_err(|e| LlmError::InvalidResponse(format!("{e}; body={text}")))?;

        let choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or(LlmError::EmptyChoices)?;

        Ok(from_openai_message(choice.message))
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::ROLE_SYSTEM => "system",
        Role::ROLE_USER => "user",
        Role::ROLE_ASSISTANT => "assistant",
        Role::ROLE_TOOL => "tool",
        Role::ROLE_UNSPECIFIED => "user",
    }
}

fn message_role(msg: &Message) -> Role {
    msg.role
        .as_ref()
        .and_then(|v| v.as_known())
        .unwrap_or(Role::ROLE_UNSPECIFIED)
}

fn to_openai_message(msg: &Message) -> Value {
    let role = role_name(message_role(msg));
    let mut m = json!({ "role": role });

    match role {
        "tool" => {
            if let Some(id) = msg.tool_call_id.as_deref() {
                m["tool_call_id"] = json!(id);
            }
            if let Some(name) = msg.name.as_deref() {
                m["name"] = json!(name);
            }
            m["content"] = json!(msg.content.as_deref().unwrap_or(""));
        }
        "assistant" => {
            if let Some(content) = msg.content.as_deref() {
                m["content"] = json!(content);
            } else if msg.tool_calls.is_empty() {
                m["content"] = json!("");
            }
            if !msg.tool_calls.is_empty() {
                m["tool_calls"] = Value::Array(
                    msg.tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id.as_deref().unwrap_or(""),
                                "type": "function",
                                "function": {
                                    "name": tc.name.as_deref().unwrap_or(""),
                                    "arguments": tc.arguments_json.as_deref().unwrap_or("{}"),
                                }
                            })
                        })
                        .collect(),
                );
            }
        }
        _ => {
            m["content"] = json!(msg.content.as_deref().unwrap_or(""));
        }
    }
    m
}

fn to_openai_tool(spec: &ToolSpec) -> Result<Value, LlmError> {
    let name = spec.name.as_deref().unwrap_or("");
    let description = spec.description.as_deref().unwrap_or("");
    let parameters_json = spec.parameters_json.as_deref().unwrap_or("{}");
    let parameters: Value = serde_json::from_str(parameters_json)
        .map_err(|e| LlmError::InvalidToolParameters(e.to_string()))?;
    Ok(json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": parameters,
        }
    }))
}

fn from_openai_message(msg: OpenAiMessage) -> AssistantTurn {
    let tool_calls = msg
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|tc| {
            ToolCall::default()
                .with_id(tc.id)
                .with_name(tc.function.name)
                .with_arguments_json(tc.function.arguments)
        })
        .collect();

    let content = msg.content.and_then(|c| {
        let t = c.trim();
        if t.is_empty() {
            None
        } else {
            Some(c)
        }
    });

    AssistantTurn {
        content,
        tool_calls,
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: String,
    function: OpenAiFunction,
}

#[derive(Debug, Deserialize, Serialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}

/// Map kit [`Message`] helpers used by the agent loop.
pub fn system_message(content: impl Into<String>) -> Message {
    Message::default()
        .with_role(Role::System)
        .with_content(content.into())
}

pub fn user_message(content: impl Into<String>) -> Message {
    Message::default()
        .with_role(Role::User)
        .with_content(content.into())
}

pub fn assistant_message(turn: &AssistantTurn) -> Message {
    let mut msg = Message::default().with_role(Role::Assistant);
    if let Some(content) = turn.content.clone() {
        msg = msg.with_content(content);
    }
    msg.tool_calls = turn.tool_calls.clone();
    msg
}

pub fn tool_message(
    tool_call_id: impl Into<String>,
    name: impl Into<String>,
    content: impl Into<String>,
) -> Message {
    Message::default()
        .with_role(Role::Tool)
        .with_tool_call_id(tool_call_id.into())
        .with_name(name.into())
        .with_content(content.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    #[test]
    fn retriable_classification() {
        assert!(LlmError::EmptyChoices.is_retriable());
        assert!(!LlmError::InvalidResponse("x".into()).is_retriable());
        assert!(!LlmError::InvalidConfig("x").is_retriable());
        assert!(LlmError::Api {
            status: 429,
            body: "slow".into()
        }
        .is_retriable());
        assert!(!LlmError::Api {
            status: 401,
            body: "nope".into()
        }
        .is_retriable());
    }

    struct ScriptedLlm {
        results: Mutex<Vec<Result<AssistantTurn, LlmError>>>,
        calls: AtomicU32,
    }

    impl Llm for ScriptedLlm {
        fn complete<'a>(
            &'a self,
            _messages: &'a [Message],
            _tools: &'a [ToolSpec],
        ) -> Pin<Box<dyn Future<Output = Result<AssistantTurn, LlmError>> + Send + 'a>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let mut results = self.results.lock().unwrap();
                results.remove(0)
            })
        }
    }

    fn test_retry_policy(max_attempts: u32) -> RetryPolicy {
        RetryPolicy::default()
            .with_max_attempts(max_attempts)
            .with_initial_backoff_ms(1)
            .with_max_backoff_ms(2)
    }

    #[tokio::test]
    async fn retries_retriable_then_succeeds() {
        let inner = ScriptedLlm {
            results: Mutex::new(vec![
                Err(LlmError::Api {
                    status: 503,
                    body: "busy".into(),
                }),
                Ok(AssistantTurn {
                    content: Some("ok".into()),
                    tool_calls: vec![],
                }),
            ]),
            calls: AtomicU32::new(0),
        };
        let llm = RetryingLlm::new(inner, test_retry_policy(3)).unwrap();

        let turn = llm.complete(&[], &[]).await.unwrap();
        assert_eq!(turn.content.as_deref(), Some("ok"));
        assert_eq!(llm.inner.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn does_not_retry_non_retriable() {
        let inner = ScriptedLlm {
            results: Mutex::new(vec![Err(LlmError::Api {
                status: 401,
                body: "auth".into(),
            })]),
            calls: AtomicU32::new(0),
        };
        let llm = RetryingLlm::new(inner, test_retry_policy(5)).unwrap();

        let err = llm.complete(&[], &[]).await.unwrap_err();
        assert!(!err.is_retriable());
        assert_eq!(llm.inner.calls.load(Ordering::SeqCst), 1);
    }
}
