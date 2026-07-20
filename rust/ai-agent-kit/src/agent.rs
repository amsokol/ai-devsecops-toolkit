//! Minimal agent loop: LLM ↔ tools until final text or max steps.

use api::aiagentkit::v1::{
    AgentLlmStep, AgentToolCall, AgentToolResult, AgentTurn, Message, RunAgent, SkillBundle,
};

use crate::llm::{
    assistant_message, system_message, tool_message, user_message, AssistantTurn, Llm, LlmError,
};
use crate::observe::{AgentObserver, NoopObserver};
use crate::skills::{self, load_skills};
use crate::tools::{ToolError, ToolRegistry};

/// Errors from [`run_agent`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("user_message is required")]
    MissingUserMessage,

    #[error("max_steps must be >= 1")]
    MissingMaxSteps,

    #[error("skill_id set but workspace_root is missing")]
    MissingWorkspaceRootForSkill,

    #[error(transparent)]
    Skills(#[from] skills::Error),

    #[error(transparent)]
    Llm(#[from] LlmError),

    #[error(transparent)]
    Tool(#[from] ToolError),

    #[error("max_steps ({0}) reached without a final assistant text")]
    MaxStepsExceeded(u32),
}

/// Format a [`SkillBundle`] into system-prompt markdown.
pub fn format_skills_prompt(bundle: &SkillBundle) -> String {
    let skill_id = bundle.skill_id.as_deref().unwrap_or("unknown");
    let mut out = format!("# Skill: {skill_id}\n");
    for file in &bundle.files {
        let name = file.name.as_deref().unwrap_or("unnamed.md");
        let content = file.content.as_deref().unwrap_or("");
        out.push_str("\n## ");
        out.push_str(name);
        out.push('\n');
        out.push_str(content);
        if !content.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// Run an agent turn: optional skills → system prompt → LLM/tool loop.
pub async fn run_agent(
    params: &RunAgent,
    llm: &dyn Llm,
    tools: &ToolRegistry,
) -> Result<AgentTurn, Error> {
    run_agent_with_observer(params, llm, tools, &NoopObserver).await
}

/// Like [`run_agent`], but emits protobuf observability events to `observer`.
pub async fn run_agent_with_observer(
    params: &RunAgent,
    llm: &dyn Llm,
    tools: &ToolRegistry,
    observer: &dyn AgentObserver,
) -> Result<AgentTurn, Error> {
    let user_message_text = params
        .user_message
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(Error::MissingUserMessage)?;

    let max_steps = match params.max_steps {
        Some(n) if n >= 1 => n,
        _ => return Err(Error::MissingMaxSteps),
    };

    let mut system_parts: Vec<String> = Vec::new();

    let skill_id = params
        .skill_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    if let Some(skill_id) = skill_id {
        let workspace_root = params
            .workspace_root
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or(Error::MissingWorkspaceRootForSkill)?;

        let bundle = load_skills(
            &api::aiagentkit::v1::LoadSkills::default()
                .with_workspace_root(workspace_root.to_owned())
                .with_skill_id(skill_id.to_owned()),
        )?;
        system_parts.push(format_skills_prompt(&bundle));
    }

    if let Some(extra) = params
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        system_parts.push(extra.to_owned());
    }

    let mut messages: Vec<Message> = Vec::new();
    if !system_parts.is_empty() {
        messages.push(system_message(system_parts.join("\n\n")));
    }
    messages.push(user_message(user_message_text.to_owned()));

    let specs = tools.specs();
    let mut steps = 0u32;

    while steps < max_steps {
        steps += 1;
        let turn: AssistantTurn = llm.complete(&messages, &specs).await?;
        messages.push(assistant_message(&turn));

        let content = turn.content.as_deref().unwrap_or("");
        observer.on_llm_step(
            &AgentLlmStep::default()
                .with_step(steps)
                .with_tool_call_count(turn.tool_calls.len() as u32)
                .with_content(content),
        );

        if turn.tool_calls.is_empty() {
            let mut result = AgentTurn::default()
                .with_final_text(turn.content.unwrap_or_default())
                .with_steps(steps);
            result.messages = messages;
            return Ok(result);
        }

        for call in &turn.tool_calls {
            let name = call.name.as_deref().unwrap_or("");
            let id = call.id.as_deref().unwrap_or("");
            let args = call.arguments_json.as_deref().unwrap_or("{}");

            observer.on_tool_call(
                &AgentToolCall::default()
                    .with_step(steps)
                    .with_tool_call_id(id)
                    .with_tool_name(name)
                    .with_arguments_json(args),
            );

            let (ok, output) = match tools.call(name, args) {
                Ok(s) => (true, s),
                Err(e) => (false, e.to_string()),
            };

            observer.on_tool_result(
                &AgentToolResult::default()
                    .with_step(steps)
                    .with_tool_call_id(id)
                    .with_tool_name(name)
                    .with_ok(ok)
                    .with_output(&output),
            );

            messages.push(tool_message(id.to_owned(), name.to_owned(), output));
        }
    }

    Err(Error::MaxStepsExceeded(max_steps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmError;
    use crate::tools::{Tool, ToolError};
    use api::aiagentkit::v1::ToolSpec;
    use std::sync::Mutex;

    struct FakeLlm {
        script: Mutex<Vec<AssistantTurn>>,
    }

    impl FakeLlm {
        fn new(script: Vec<AssistantTurn>) -> Self {
            Self {
                script: Mutex::new(script),
            }
        }
    }

    impl Llm for FakeLlm {
        fn complete<'a>(
            &'a self,
            _messages: &'a [Message],
            _tools: &'a [ToolSpec],
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<AssistantTurn, LlmError>> + Send + 'a>,
        > {
            Box::pin(async move {
                let mut script = self.script.lock().unwrap();
                if script.is_empty() {
                    return Ok(AssistantTurn {
                        content: Some("done".into()),
                        tool_calls: vec![],
                    });
                }
                Ok(script.remove(0))
            })
        }
    }

    struct EchoTool;

    impl Tool for EchoTool {
        fn spec(&self) -> ToolSpec {
            ToolSpec::default()
                .with_name("echo")
                .with_description("Echo arguments")
                .with_parameters_json(
                    r#"{"type":"object","properties":{"text":{"type":"string"}}}"#,
                )
        }

        fn call(&self, arguments_json: &str) -> Result<String, ToolError> {
            Ok(format!("echoed:{arguments_json}"))
        }
    }

    #[derive(Default)]
    struct RecordingObserver {
        llm_steps: Mutex<Vec<AgentLlmStep>>,
        tool_calls: Mutex<Vec<AgentToolCall>>,
        tool_results: Mutex<Vec<AgentToolResult>>,
    }

    impl AgentObserver for RecordingObserver {
        fn on_llm_step(&self, event: &AgentLlmStep) {
            self.llm_steps.lock().unwrap().push(event.clone());
        }

        fn on_tool_call(&self, event: &AgentToolCall) {
            self.tool_calls.lock().unwrap().push(event.clone());
        }

        fn on_tool_result(&self, event: &AgentToolResult) {
            self.tool_results.lock().unwrap().push(event.clone());
        }
    }

    #[tokio::test]
    async fn runs_tool_then_final_text() {
        let llm = FakeLlm::new(vec![
            AssistantTurn {
                content: None,
                tool_calls: vec![api::aiagentkit::v1::ToolCall::default()
                    .with_id("call_1")
                    .with_name("echo")
                    .with_arguments_json(r#"{"text":"hi"}"#)],
            },
            AssistantTurn {
                content: Some("all good".into()),
                tool_calls: vec![],
            },
        ]);

        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);

        let params = RunAgent::default()
            .with_user_message("please echo")
            .with_max_steps(4);

        let turn = run_agent(&params, &llm, &registry).await.unwrap();
        assert_eq!(turn.final_text.as_deref(), Some("all good"));
        assert_eq!(turn.steps, Some(2));
        assert!(turn.messages.len() >= 4);
    }

    #[tokio::test]
    async fn emits_observer_events_for_tool_loop() {
        let llm = FakeLlm::new(vec![
            AssistantTurn {
                content: None,
                tool_calls: vec![api::aiagentkit::v1::ToolCall::default()
                    .with_id("call_1")
                    .with_name("echo")
                    .with_arguments_json(r#"{"text":"hi"}"#)],
            },
            AssistantTurn {
                content: Some("all good".into()),
                tool_calls: vec![],
            },
        ]);

        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        let observer = RecordingObserver::default();

        let params = RunAgent::default()
            .with_user_message("please echo")
            .with_max_steps(4);

        let _turn = run_agent_with_observer(&params, &llm, &registry, &observer)
            .await
            .unwrap();

        let llm_steps = observer.llm_steps.lock().unwrap();
        assert_eq!(llm_steps.len(), 2);
        assert_eq!(llm_steps[0].tool_call_count, Some(1));
        assert_eq!(llm_steps[1].tool_call_count, Some(0));
        assert_eq!(llm_steps[1].content.as_deref(), Some("all good"));

        let calls = observer.tool_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_name.as_deref(), Some("echo"));
        assert_eq!(calls[0].arguments_json.as_deref(), Some(r#"{"text":"hi"}"#));

        let results = observer.tool_results.lock().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ok, Some(true));
        assert_eq!(
            results[0].output.as_deref(),
            Some(r#"echoed:{"text":"hi"}"#)
        );
    }

    #[tokio::test]
    async fn formats_skills_into_system_prompt() {
        use std::fs;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let skill = dir.path().join(".skills").join("demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "Be concise.\n").unwrap();

        let llm = FakeLlm::new(vec![AssistantTurn {
            content: Some("ok".into()),
            tool_calls: vec![],
        }]);

        let registry = ToolRegistry::new();
        let params = RunAgent::default()
            .with_workspace_root(dir.path().to_string_lossy())
            .with_skill_id("demo")
            .with_user_message("hi")
            .with_max_steps(2);

        let turn = run_agent(&params, &llm, &registry).await.unwrap();
        let system = turn.messages[0].content.as_deref().unwrap_or("");
        assert!(system.contains("# Skill: demo"));
        assert!(system.contains("Be concise."));
    }
}
