//! Workspace-sandboxed `run_command` tool (argv only; allowlisted programs).

use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use api::aiagentkit::v1::{CommandResult, RunCommand, ShellToolConfig, ToolSpec};

use crate::fs_tools::resolve_under_root;
use crate::tools::{Tool, ToolError, ToolRegistry};

/// Default allowlist for depbot-oriented CLIs (caller may override via config).
pub const DEFAULT_SHELL_ALLOWLIST: &[&str] = &[
    "go", "npm", "npx", "node", "yarn", "pnpm", "git", "gh", "cargo", "rustc", "rg", "bazel",
];

/// Suggested defaults when the caller builds a [`ShellToolConfig`].
pub const DEFAULT_SHELL_TIMEOUT_MS: u32 = 60_000;
pub const DEFAULT_SHELL_MAX_TIMEOUT_MS: u32 = 300_000;
pub const DEFAULT_SHELL_MAX_OUTPUT_BYTES: u64 = 256 * 1024;

const RUN_COMMAND_PARAMS: &str = r#"{
  "type": "object",
  "properties": {
    "program": {
      "type": "string",
      "description": "Program basename only (must be allowlisted); resolved via PATH. No shell."
    },
    "args": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Arguments passed to the program (not interpreted by a shell)"
    },
    "cwd": {
      "type": "string",
      "description": "Working directory relative to the workspace root (\".\" = workspace root)"
    },
    "timeout_ms": {
      "type": "integer",
      "description": "Optional timeout in milliseconds (capped by tool config)"
    }
  },
  "required": ["program"],
  "additionalProperties": false
}"#;

/// Build a [`ShellToolConfig`] with workspace root and recommended defaults.
pub fn default_shell_tool_config(workspace_root: impl Into<String>) -> ShellToolConfig {
    let mut cfg = ShellToolConfig::default()
        .with_workspace_root(workspace_root)
        .with_default_timeout_ms(DEFAULT_SHELL_TIMEOUT_MS)
        .with_max_timeout_ms(DEFAULT_SHELL_MAX_TIMEOUT_MS)
        .with_max_output_bytes(DEFAULT_SHELL_MAX_OUTPUT_BYTES);
    cfg.allowed_programs = DEFAULT_SHELL_ALLOWLIST
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    cfg
}

/// Register sandboxed `run_command` for the given config.
pub fn register_workspace_shell_tool(
    registry: &mut ToolRegistry,
    config: ShellToolConfig,
) -> Result<(), ToolError> {
    let tool = RunCommandTool::try_new(config)?;
    registry.register(tool);
    Ok(())
}

struct RunCommandTool {
    workspace_root: PathBuf,
    allowlist: HashSet<String>,
    default_timeout_ms: u32,
    max_timeout_ms: u32,
    max_output_bytes: u64,
}

impl RunCommandTool {
    fn try_new(config: ShellToolConfig) -> Result<Self, ToolError> {
        let workspace_root = config
            .workspace_root
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::Failed {
                name: "run_command".into(),
                message: "ShellToolConfig.workspace_root is required".into(),
            })?;
        let workspace_root = PathBuf::from(workspace_root)
            .canonicalize()
            .map_err(|e| ToolError::Failed {
                name: "run_command".into(),
                message: format!("canonicalize workspace_root: {e}"),
            })?;

        let default_timeout_ms = match config.default_timeout_ms {
            Some(n) if n >= 1 => n,
            _ => {
                return Err(ToolError::Failed {
                    name: "run_command".into(),
                    message: "ShellToolConfig.default_timeout_ms must be >= 1".into(),
                });
            }
        };
        let max_timeout_ms = match config.max_timeout_ms {
            Some(n) if n >= 1 => n,
            _ => {
                return Err(ToolError::Failed {
                    name: "run_command".into(),
                    message: "ShellToolConfig.max_timeout_ms must be >= 1".into(),
                });
            }
        };
        if default_timeout_ms > max_timeout_ms {
            return Err(ToolError::Failed {
                name: "run_command".into(),
                message: "ShellToolConfig.default_timeout_ms must be <= max_timeout_ms".into(),
            });
        }
        let max_output_bytes = match config.max_output_bytes {
            Some(n) if n >= 1 => n,
            _ => {
                return Err(ToolError::Failed {
                    name: "run_command".into(),
                    message: "ShellToolConfig.max_output_bytes must be >= 1".into(),
                });
            }
        };

        let mut allowlist = HashSet::new();
        for prog in &config.allowed_programs {
            let name = prog.trim();
            if name.is_empty() {
                continue;
            }
            if !is_safe_program_basename(name) {
                return Err(ToolError::Failed {
                    name: "run_command".into(),
                    message: format!(
                        "ShellToolConfig.allowed_programs entry must be a basename: {name:?}"
                    ),
                });
            }
            allowlist.insert(name.to_ascii_lowercase());
        }

        Ok(Self {
            workspace_root,
            allowlist,
            default_timeout_ms,
            max_timeout_ms,
            max_output_bytes,
        })
    }
}

impl Tool for RunCommandTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::default()
            .with_name("run_command")
            .with_description(
                "Run an allowlisted program with argv (no shell). cwd must be under the workspace. \
                 Captures stdout/stderr with timeout and size limits.",
            )
            .with_parameters_json(RUN_COMMAND_PARAMS)
    }

    fn call(&self, arguments_json: &str) -> Result<String, ToolError> {
        let input: RunCommand =
            serde_json::from_str(arguments_json).map_err(|e| ToolError::Failed {
                name: "run_command".into(),
                message: format!("invalid arguments: {e}"),
            })?;

        let program = input
            .program
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::Failed {
                name: "run_command".into(),
                message: "program is required".into(),
            })?;

        if !is_safe_program_basename(program) {
            return Err(ToolError::Failed {
                name: "run_command".into(),
                message: "program must be a basename (no path separators)".into(),
            });
        }

        let program_key = program.to_ascii_lowercase();
        if !self.allowlist.contains(&program_key) {
            return Err(ToolError::Failed {
                name: "run_command".into(),
                message: format!("program `{program}` is not allowlisted"),
            });
        }

        for arg in &input.args {
            if arg.contains('\0') {
                return Err(ToolError::Failed {
                    name: "run_command".into(),
                    message: "args must not contain NUL bytes".into(),
                });
            }
        }

        if program_key == "git" {
            if let Err(message) = check_git_guardrails(&input.args) {
                return Err(ToolError::Failed {
                    name: "run_command".into(),
                    message,
                });
            }
        }

        let cwd_rel = input
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(".");
        let cwd_abs = resolve_under_root(&self.workspace_root, cwd_rel).map_err(|message| {
            ToolError::Failed {
                name: "run_command".into(),
                message,
            }
        })?;
        if !cwd_abs.is_dir() {
            return Err(ToolError::Failed {
                name: "run_command".into(),
                message: format!("cwd `{cwd_rel}` is not a directory"),
            });
        }

        let timeout_ms = match input.timeout_ms {
            Some(n) if n >= 1 => n.min(self.max_timeout_ms),
            Some(_) => {
                return Err(ToolError::Failed {
                    name: "run_command".into(),
                    message: "timeout_ms must be >= 1 when set".into(),
                });
            }
            None => self.default_timeout_ms,
        };

        let started = Instant::now();
        let mut child = Command::new(program)
            .args(&input.args)
            .current_dir(&cwd_abs)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::Failed {
                name: "run_command".into(),
                message: format!("spawn `{program}`: {e}"),
            })?;

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let stdout_handle = thread::spawn(move || read_pipe(stdout_pipe));
        let stderr_handle = thread::spawn(move || read_pipe(stderr_pipe));

        let timeout = Duration::from_millis(u64::from(timeout_ms));
        let mut timed_out = false;
        let exit_code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code().unwrap_or(-1),
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        timed_out = true;
                        let _ = child.kill();
                        let _ = child.wait();
                        break -1;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = join_bytes(stdout_handle);
                    let _ = join_bytes(stderr_handle);
                    return Err(ToolError::Failed {
                        name: "run_command".into(),
                        message: format!("wait `{program}`: {e}"),
                    });
                }
            }
        };

        let stdout_raw = join_bytes(stdout_handle).unwrap_or_default();
        let stderr_raw = join_bytes(stderr_handle).unwrap_or_default();
        let duration_ms = started.elapsed().as_millis() as u64;

        let (stdout, stderr, truncated) =
            truncate_outputs(&stdout_raw, &stderr_raw, self.max_output_bytes);

        let result = CommandResult::default()
            .with_exit_code(exit_code)
            .with_stdout(stdout)
            .with_stderr(stderr)
            .with_timed_out(timed_out)
            .with_truncated(truncated)
            .with_duration_ms(duration_ms)
            .with_program(program)
            .with_cwd(cwd_rel);

        serde_json::to_string(&result).map_err(|e| ToolError::Failed {
            name: "run_command".into(),
            message: format!("serialize result: {e}"),
        })
    }
}

fn is_safe_program_basename(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return false;
    }
    // Reject drive-relative Windows forms like "C:foo".
    if name.len() >= 2 && name.as_bytes()[1] == b':' {
        return false;
    }
    true
}

fn check_git_guardrails(args: &[String]) -> Result<(), String> {
    let lower: Vec<String> = args.iter().map(|a| a.to_ascii_lowercase()).collect();
    if lower.iter().any(|a| a == "--force" || a == "-f" || a == "--force-with-lease")
        && lower.iter().any(|a| a == "push")
    {
        return Err("git force-push is blocked by guardrails".into());
    }
    if lower.iter().any(|a| a == "reset") && lower.iter().any(|a| a == "--hard") {
        return Err("git reset --hard is blocked by guardrails".into());
    }
    if lower.iter().any(|a| a == "clean") {
        return Err("git clean is blocked by guardrails".into());
    }
    if lower.iter().any(|a| a == "filter-branch" || a == "filter-repo") {
        return Err("git history rewrite commands are blocked by guardrails".into());
    }
    Ok(())
}

fn read_pipe(pipe: Option<impl Read>) -> Vec<u8> {
    let Some(mut pipe) = pipe else {
        return Vec::new();
    };
    let mut buf = Vec::new();
    let _ = pipe.read_to_end(&mut buf);
    buf
}

fn join_bytes(handle: thread::JoinHandle<Vec<u8>>) -> Result<Vec<u8>, ()> {
    handle.join().map_err(|_| ())
}

fn truncate_outputs(stdout: &[u8], stderr: &[u8], max_total: u64) -> (String, String, bool) {
    let max = max_total as usize;
    let mut truncated = false;
    let mut remaining = max;

    let (out, out_trunc) = take_lossy(stdout, remaining);
    if out_trunc {
        truncated = true;
    }
    remaining = remaining.saturating_sub(out.len());

    let (err, err_trunc) = take_lossy(stderr, remaining);
    if err_trunc {
        truncated = true;
    }

    (out, err, truncated)
}

fn take_lossy(bytes: &[u8], max: usize) -> (String, bool) {
    if bytes.len() <= max {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }
    let slice = &bytes[..max];
    let mut s = String::from_utf8_lossy(slice).into_owned();
    s.push_str("\n…[truncated]");
    (s, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn cfg_for(dir: &Path, allow: &[&str]) -> ShellToolConfig {
        let mut cfg = ShellToolConfig::default()
            .with_workspace_root(dir.to_string_lossy())
            .with_default_timeout_ms(5_000)
            .with_max_timeout_ms(10_000)
            .with_max_output_bytes(64 * 1024);
        cfg.allowed_programs = allow.iter().map(|s| (*s).to_owned()).collect();
        cfg
    }

    #[test]
    fn rejects_non_allowlisted_program() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ToolRegistry::new();
        register_workspace_shell_tool(&mut reg, cfg_for(dir.path(), &["echo"])).unwrap();
        let err = reg
            .call("run_command", r#"{"program":"rm","args":["-rf","/"]}"#)
            .unwrap_err();
        assert!(err.to_string().contains("not allowlisted"));
    }

    #[test]
    fn rejects_path_program() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ToolRegistry::new();
        register_workspace_shell_tool(&mut reg, cfg_for(dir.path(), &["echo"])).unwrap();
        let err = reg
            .call("run_command", r#"{"program":"/bin/echo","args":["hi"]}"#)
            .unwrap_err();
        assert!(err.to_string().contains("basename"));
    }

    #[test]
    fn rejects_cwd_escape() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ToolRegistry::new();
        register_workspace_shell_tool(&mut reg, cfg_for(dir.path(), &["echo"])).unwrap();
        let err = reg
            .call(
                "run_command",
                r#"{"program":"echo","args":["hi"],"cwd":".."}"#,
            )
            .unwrap_err();
        assert!(err.to_string().contains("..") || err.to_string().contains("escape"));
    }

    #[test]
    fn blocks_git_reset_hard() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ToolRegistry::new();
        register_workspace_shell_tool(&mut reg, cfg_for(dir.path(), &["git"])).unwrap();
        let err = reg
            .call(
                "run_command",
                r#"{"program":"git","args":["reset","--hard","HEAD"]}"#,
            )
            .unwrap_err();
        assert!(err.to_string().contains("reset --hard"));
    }

    #[test]
    fn runs_echo_ok() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("marker.txt"), "x").unwrap();
        let mut reg = ToolRegistry::new();
        // Prefer a program available on CI: use `git` with --version (usually present).
        register_workspace_shell_tool(&mut reg, cfg_for(dir.path(), &["git"])).unwrap();
        let out = reg
            .call(
                "run_command",
                r#"{"program":"git","args":["--version"],"cwd":"."}"#,
            )
            .unwrap();
        let parsed: CommandResult = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.timed_out, Some(false));
        assert_eq!(parsed.exit_code, Some(0));
        let stdout = parsed.stdout.as_deref().unwrap_or("");
        assert!(stdout.to_ascii_lowercase().contains("git"), "{stdout}");
    }

    #[test]
    fn times_out_long_running_process() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ToolRegistry::new();
        let mut cfg = cfg_for(dir.path(), &["sleep"]);
        cfg = cfg.with_default_timeout_ms(200).with_max_timeout_ms(1_000);
        // sleep may not exist on Windows; skip if spawn fails for missing binary.
        if register_workspace_shell_tool(&mut reg, cfg).is_err() {
            return;
        }
        match reg.call(
            "run_command",
            r#"{"program":"sleep","args":["5"],"timeout_ms":200}"#,
        ) {
            Ok(out) => {
                let parsed: CommandResult = serde_json::from_str(&out).unwrap();
                assert_eq!(parsed.timed_out, Some(true));
            }
            Err(e) => {
                // Program not on PATH (e.g. Windows without sleep).
                assert!(
                    e.to_string().contains("spawn") || e.to_string().contains("not allowlisted"),
                    "{e}"
                );
            }
        }
    }

    #[test]
    fn default_config_has_allowlist() {
        let cfg = default_shell_tool_config("/tmp/ws");
        assert!(!cfg.allowed_programs.is_empty());
        assert_eq!(cfg.default_timeout_ms, Some(DEFAULT_SHELL_TIMEOUT_MS));
    }
}
