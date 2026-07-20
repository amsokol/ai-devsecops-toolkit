//! Observability hooks for the agent loop (protobuf events).

use api::aiagentkit::v1::{AgentLlmStep, AgentToolCall, AgentToolResult};

/// Receives structured agent-loop events as protobuf messages.
///
/// Default methods are no-ops so callers can implement only what they need.
pub trait AgentObserver: Send + Sync {
    fn on_llm_step(&self, _event: &AgentLlmStep) {}
    fn on_tool_call(&self, _event: &AgentToolCall) {}
    fn on_tool_result(&self, _event: &AgentToolResult) {}
}

/// Observer that discards all events.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopObserver;

impl AgentObserver for NoopObserver {}

/// Writes truncated agent events to stderr (CLI `--verbose`).
#[derive(Debug, Clone, Copy)]
pub struct StderrObserver {
    /// Max characters printed for args/content/output payloads.
    pub max_payload_chars: usize,
}

impl Default for StderrObserver {
    fn default() -> Self {
        Self {
            max_payload_chars: 512,
        }
    }
}

impl StderrObserver {
    #[must_use]
    pub fn new(max_payload_chars: usize) -> Self {
        Self { max_payload_chars }
    }

    fn trunc<'a>(&self, s: &'a str) -> Truncated<'a> {
        Truncated {
            text: s,
            max: self.max_payload_chars,
        }
    }
}

struct Truncated<'a> {
    text: &'a str,
    max: usize,
}

impl std::fmt::Display for Truncated<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut chars = self.text.chars();
        let mut shown = String::new();
        for _ in 0..self.max {
            match chars.next() {
                Some(c) => shown.push(c),
                None => return write!(f, "{shown}"),
            }
        }
        match chars.next() {
            None => write!(f, "{shown}"),
            Some(_) => {
                let rest = 1 + chars.count();
                write!(f, "{shown}…(+{rest} chars)")
            }
        }
    }
}

impl AgentObserver for StderrObserver {
    fn on_llm_step(&self, event: &AgentLlmStep) {
        let step = event.step.unwrap_or(0);
        let n = event.tool_call_count.unwrap_or(0);
        let content = event.content.as_deref().unwrap_or("");
        if content.is_empty() {
            eprintln!("[agent] step={step} llm tool_calls={n}");
        } else {
            eprintln!(
                "[agent] step={step} llm tool_calls={n} content={}",
                self.trunc(content)
            );
        }
    }

    fn on_tool_call(&self, event: &AgentToolCall) {
        let step = event.step.unwrap_or(0);
        let name = event.tool_name.as_deref().unwrap_or("?");
        let id = event.tool_call_id.as_deref().unwrap_or("");
        let args = event.arguments_json.as_deref().unwrap_or("{}");
        eprintln!(
            "[agent] step={step} tool_call name={name} id={id} args={}",
            self.trunc(args)
        );
    }

    fn on_tool_result(&self, event: &AgentToolResult) {
        let step = event.step.unwrap_or(0);
        let name = event.tool_name.as_deref().unwrap_or("?");
        let id = event.tool_call_id.as_deref().unwrap_or("");
        let ok = event.ok.unwrap_or(false);
        let output = event.output.as_deref().unwrap_or("");
        let status = if ok { "ok" } else { "err" };
        eprintln!(
            "[agent] step={step} tool_result name={name} id={id} {status} output={}",
            self.trunc(output)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_payloads() {
        let obs = StderrObserver::new(5);
        let t = obs.trunc("abcdefghij");
        assert_eq!(t.to_string(), "abcde…(+5 chars)");
    }

    #[test]
    fn leaves_short_payloads() {
        let obs = StderrObserver::new(5);
        assert_eq!(obs.trunc("hi").to_string(), "hi");
    }
}
