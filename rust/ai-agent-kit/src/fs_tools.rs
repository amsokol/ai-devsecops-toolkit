//! Workspace-sandboxed filesystem tools: `read_file` and `list_dir`.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use api::aiagentkit::v1::{
    DirEntry, DirListing, FileContent, ListDir, ReadFile, ToolSpec,
};

use crate::tools::{Tool, ToolError, ToolRegistry};

/// Max bytes for [`read_file`] (256 KiB).
pub const MAX_READ_FILE_BYTES: u64 = 256 * 1024;

const READ_FILE_PARAMS: &str = r#"{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "Path relative to the workspace root"
    }
  },
  "required": ["path"],
  "additionalProperties": false
}"#;

const LIST_DIR_PARAMS: &str = r#"{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "description": "Directory path relative to the workspace root (\".\" = workspace root)"
    }
  },
  "required": ["path"],
  "additionalProperties": false
}"#;

/// Register sandboxed `read_file` and `list_dir` for `workspace_root`.
pub fn register_workspace_fs_tools(
    registry: &mut ToolRegistry,
    workspace_root: impl Into<PathBuf>,
) {
    let root = Arc::new(workspace_root.into());
    registry.register(ReadFileTool {
        workspace_root: Arc::clone(&root),
    });
    registry.register(ListDirTool {
        workspace_root: root,
    });
}

struct ReadFileTool {
    workspace_root: Arc<PathBuf>,
}

impl Tool for ReadFileTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::default()
            .with_name("read_file")
            .with_description(
                "Read a UTF-8 text file under the workspace. Path must be relative; max 256KiB.",
            )
            .with_parameters_json(READ_FILE_PARAMS)
    }

    fn call(&self, arguments_json: &str) -> Result<String, ToolError> {
        let input: ReadFile = serde_json::from_str(arguments_json).map_err(|e| ToolError::Failed {
            name: "read_file".into(),
            message: format!("invalid arguments: {e}"),
        })?;
        let req_path = input.path.as_deref().unwrap_or("").trim();
        if req_path.is_empty() {
            return Err(ToolError::Failed {
                name: "read_file".into(),
                message: "path is required".into(),
            });
        }

        let abs = resolve_under_root(&self.workspace_root, req_path).map_err(|message| {
            ToolError::Failed {
                name: "read_file".into(),
                message,
            }
        })?;

        let meta = fs::metadata(&abs).map_err(|e| ToolError::Failed {
            name: "read_file".into(),
            message: format!("stat {req_path}: {e}"),
        })?;
        if !meta.is_file() {
            return Err(ToolError::Failed {
                name: "read_file".into(),
                message: format!("{req_path} is not a file"),
            });
        }
        if meta.len() > MAX_READ_FILE_BYTES {
            return Err(ToolError::Failed {
                name: "read_file".into(),
                message: format!(
                    "file exceeds max size ({MAX_READ_FILE_BYTES} bytes): {} bytes",
                    meta.len()
                ),
            });
        }

        let content = fs::read_to_string(&abs).map_err(|e| ToolError::Failed {
            name: "read_file".into(),
            message: format!("read {req_path}: {e}"),
        })?;

        let out = FileContent::default()
            .with_path(req_path.to_owned())
            .with_content(content);
        serde_json::to_string(&out).map_err(|e| ToolError::Failed {
            name: "read_file".into(),
            message: format!("serialize result: {e}"),
        })
    }
}

struct ListDirTool {
    workspace_root: Arc<PathBuf>,
}

impl Tool for ListDirTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec::default()
            .with_name("list_dir")
            .with_description(
                "List entries in a workspace directory (non-recursive). Path must be relative.",
            )
            .with_parameters_json(LIST_DIR_PARAMS)
    }

    fn call(&self, arguments_json: &str) -> Result<String, ToolError> {
        let input: ListDir = serde_json::from_str(arguments_json).map_err(|e| ToolError::Failed {
            name: "list_dir".into(),
            message: format!("invalid arguments: {e}"),
        })?;
        let req_path = input.path.as_deref().unwrap_or("").trim();
        if req_path.is_empty() {
            return Err(ToolError::Failed {
                name: "list_dir".into(),
                message: "path is required".into(),
            });
        }

        let abs = resolve_under_root(&self.workspace_root, req_path).map_err(|message| {
            ToolError::Failed {
                name: "list_dir".into(),
                message,
            }
        })?;

        let meta = fs::metadata(&abs).map_err(|e| ToolError::Failed {
            name: "list_dir".into(),
            message: format!("stat {req_path}: {e}"),
        })?;
        if !meta.is_dir() {
            return Err(ToolError::Failed {
                name: "list_dir".into(),
                message: format!("{req_path} is not a directory"),
            });
        }

        let mut entries: Vec<DirEntry> = Vec::new();
        let rd = fs::read_dir(&abs).map_err(|e| ToolError::Failed {
            name: "list_dir".into(),
            message: format!("read_dir {req_path}: {e}"),
        })?;
        for item in rd {
            let Ok(item) = item else {
                continue;
            };
            let Ok(name) = item.file_name().into_string() else {
                continue;
            };
            let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(DirEntry::default().with_name(name).with_is_dir(is_dir));
        }
        entries.sort_by(|a, b| {
            a.name
                .as_deref()
                .unwrap_or("")
                .cmp(b.name.as_deref().unwrap_or(""))
        });

        let mut out = DirListing::default().with_path(req_path.to_owned());
        out.entries = entries;
        serde_json::to_string(&out).map_err(|e| ToolError::Failed {
            name: "list_dir".into(),
            message: format!("serialize result: {e}"),
        })
    }
}

/// Resolve `rel` under `workspace_root`, rejecting absolute paths, `..`, and symlink escapes.
///
/// Works on Windows and Unix: containment checks normalize verbatim (`\\?\`) prefixes
/// so `canonicalize` results compare correctly.
fn resolve_under_root(workspace_root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err("path must be relative to the workspace root".into());
    }

    let root = workspace_root
        .canonicalize()
        .map_err(|e| format!("canonicalize workspace_root: {e}"))?;

    let mut resolved = root.clone();
    for component in rel_path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => resolved.push(part),
            Component::ParentDir => {
                return Err("path must not contain '..'".into());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("path must be relative to the workspace root".into());
            }
        }
    }

    if resolved.exists() {
        let canon = resolved
            .canonicalize()
            .map_err(|e| format!("canonicalize path: {e}"))?;
        if !path_is_within(&canon, &root) {
            return Err("path escapes the workspace root".into());
        }
        return Ok(canon);
    }

    if !path_is_within(&resolved, &root) {
        return Err("path escapes the workspace root".into());
    }
    Ok(resolved)
}

/// True if `path` is `root` or a descendant (after stripping Windows verbatim prefixes).
fn path_is_within(path: &Path, root: &Path) -> bool {
    let path = strip_verbatim_prefix(path);
    let root = strip_verbatim_prefix(root);
    path.starts_with(&root)
}

/// Strip Windows `\\?\` / `\\?\UNC\` prefixes so path prefix checks are stable.
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.as_os_str().to_string_lossy();
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            if let Some(unc) = rest.strip_prefix(r"UNC\") {
                return PathBuf::from(format!(r"\\{unc}"));
            }
            return PathBuf::from(rest);
        }
        return path.to_path_buf();
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn registry_in(dir: &Path) -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        register_workspace_fs_tools(&mut reg, dir);
        reg
    }

    #[test]
    fn read_file_ok() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hello.txt"), "hi\n").unwrap();
        let reg = registry_in(dir.path());
        let out = reg
            .call("read_file", r#"{"path":"hello.txt"}"#)
            .unwrap();
        let parsed: FileContent = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.path.as_deref(), Some("hello.txt"));
        assert_eq!(parsed.content.as_deref(), Some("hi\n"));
    }

    #[test]
    fn list_dir_ok() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        let reg = registry_in(dir.path());
        let out = reg.call("list_dir", r#"{"path":"."}"#).unwrap();
        let parsed: DirListing = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.path.as_deref(), Some("."));
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].name.as_deref(), Some("a.txt"));
        assert_eq!(parsed.entries[0].is_dir, Some(false));
        assert_eq!(parsed.entries[1].name.as_deref(), Some("sub"));
        assert_eq!(parsed.entries[1].is_dir, Some(true));
    }

    #[test]
    fn rejects_parent_escape() {
        let dir = tempfile::tempdir().unwrap();
        let reg = registry_in(dir.path());
        let err = reg
            .call("read_file", r#"{"path":"../secret"}"#)
            .unwrap_err();
        assert!(err.to_string().contains(".."), "{err}");
    }

    #[test]
    fn rejects_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let reg = registry_in(dir.path());
        let abs = dir.path().join("hello.txt");
        fs::write(&abs, "x").unwrap();
        // JSON-escape so Windows `\` paths are valid.
        let args = serde_json::json!({ "path": abs.to_string_lossy() }).to_string();
        let err = reg.call("read_file", &args).unwrap_err();
        assert!(err.to_string().contains("relative"), "{err}");
    }

    #[test]
    fn missing_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let reg = registry_in(dir.path());
        let err = reg
            .call("read_file", r#"{"path":"nope.txt"}"#)
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("stat")
                || msg.contains("No such")
                || msg.contains("cannot find")
                || msg.contains("os error"),
            "{err}"
        );
    }

    #[test]
    fn oversize_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        let mut f = fs::File::create(&path).unwrap();
        let chunk = vec![b'x'; 4096];
        let mut written = 0u64;
        while written <= MAX_READ_FILE_BYTES {
            f.write_all(&chunk).unwrap();
            written += chunk.len() as u64;
        }
        drop(f);

        let reg = registry_in(dir.path());
        let err = reg
            .call("read_file", r#"{"path":"big.bin"}"#)
            .unwrap_err();
        assert!(err.to_string().contains("max size"), "{err}");
    }
}
