//! Load `.depbot/tools.yaml` as [`ToolsFile`] and map to kit [`ShellToolConfig`].

use std::fs;
use std::path::Path;

use api::aiagentkit::v1::ShellToolConfig;
use api::depbot::v1::ToolsFile;

/// Default relative path under a workspace root.
pub const DEFAULT_TOOLS_CONFIG: &str = ".depbot/tools.yaml";

/// Errors from loading or mapping depbot tools config.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("parse YAML as JSON value: {0}")]
    Yaml(String),

    #[error("decode ToolsFile from JSON: {0}")]
    ProtoJson(String),

    #[error("{0}")]
    Invalid(String),
}

/// Load `depbot.v1.ToolsFile` from a YAML file (YAML → JSON → protobuf/serde).
pub fn load_tools_file(path: impl AsRef<Path>) -> Result<ToolsFile, Error> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;

    let value: serde_json::Value =
        serde_yaml::from_str(&text).map_err(|e| Error::Yaml(e.to_string()))?;

    let file: ToolsFile =
        serde_json::from_value(value).map_err(|e| Error::ProtoJson(e.to_string()))?;

    validate_tools_file(&file)?;
    Ok(file)
}

fn validate_tools_file(file: &ToolsFile) -> Result<(), Error> {
    match file.version {
        Some(v) if v >= 1 => {}
        Some(v) => {
            return Err(Error::Invalid(format!(
                "ToolsFile.version must be >= 1, got {v}"
            )));
        }
        None => return Err(Error::Invalid("ToolsFile.version is required".into())),
    }

    let shell = file.shell.as_option().ok_or_else(|| {
        Error::Invalid("ToolsFile.shell is required".into())
    })?;

    match shell.default_timeout_ms {
        Some(n) if n >= 1 => {}
        _ => {
            return Err(Error::Invalid(
                "ToolsFile.shell.default_timeout_ms must be >= 1".into(),
            ));
        }
    }
    match shell.max_timeout_ms {
        Some(n) if n >= 1 => {}
        _ => {
            return Err(Error::Invalid(
                "ToolsFile.shell.max_timeout_ms must be >= 1".into(),
            ));
        }
    }
    if shell.default_timeout_ms.unwrap_or(0) > shell.max_timeout_ms.unwrap_or(0) {
        return Err(Error::Invalid(
            "ToolsFile.shell.default_timeout_ms must be <= max_timeout_ms".into(),
        ));
    }
    match shell.max_output_bytes {
        Some(n) if n >= 1 => {}
        _ => {
            return Err(Error::Invalid(
                "ToolsFile.shell.max_output_bytes must be >= 1".into(),
            ));
        }
    }

    if let Some(curl) = file.curl.as_option() {
        for prefix in &curl.url_prefixes {
            let p = prefix.trim();
            if p.is_empty() {
                continue;
            }
            if !p.starts_with("https://") {
                return Err(Error::Invalid(format!(
                    "ToolsFile.curl.url_prefixes must start with https://: {p:?}"
                )));
            }
        }
    }

    Ok(())
}

/// Map a loaded [`ToolsFile`] into kit [`ShellToolConfig`] for `workspace_root`.
pub fn shell_tool_config_from_tools_file(
    file: &ToolsFile,
    workspace_root: impl Into<String>,
) -> Result<ShellToolConfig, Error> {
    validate_tools_file(file)?;
    let shell = file
        .shell
        .as_option()
        .ok_or_else(|| Error::Invalid("ToolsFile.shell is required".into()))?;

    let default_timeout_ms = shell.default_timeout_ms.ok_or_else(|| {
        Error::Invalid("ToolsFile.shell.default_timeout_ms must be >= 1".into())
    })?;
    let max_timeout_ms = shell.max_timeout_ms.ok_or_else(|| {
        Error::Invalid("ToolsFile.shell.max_timeout_ms must be >= 1".into())
    })?;
    let max_output_bytes = shell.max_output_bytes.ok_or_else(|| {
        Error::Invalid("ToolsFile.shell.max_output_bytes must be >= 1".into())
    })?;

    let mut cfg = ShellToolConfig::default()
        .with_workspace_root(workspace_root)
        .with_default_timeout_ms(default_timeout_ms)
        .with_max_timeout_ms(max_timeout_ms)
        .with_max_output_bytes(max_output_bytes);

    cfg.allowed_programs = shell.allowed_programs.clone();
    if let Some(curl) = file.curl.as_option() {
        cfg.curl_url_prefixes = curl
            .url_prefixes
            .iter()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
    }

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    #[test]
    fn loads_example_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tools.yaml");
        let mut f = fs::File::create(&path).unwrap();
        write!(
            f,
            r#"
version: 1
shell:
  allowed_programs: [git, curl]
  default_timeout_ms: 1000
  max_timeout_ms: 2000
  max_output_bytes: 4096
curl:
  url_prefixes:
    - https://registry.bazel.build/
"#
        )
        .unwrap();

        let file = load_tools_file(&path).unwrap();
        assert_eq!(file.version, Some(1));
        let shell = file.shell.as_option().unwrap();
        assert_eq!(shell.allowed_programs, vec!["git", "curl"]);

        let cfg = shell_tool_config_from_tools_file(&file, "/tmp/ws").unwrap();
        assert_eq!(cfg.allowed_programs, vec!["git", "curl"]);
        assert_eq!(
            cfg.curl_url_prefixes,
            vec!["https://registry.bazel.build/".to_owned()]
        );
        assert_eq!(cfg.default_timeout_ms, Some(1000));
    }

    #[test]
    fn rejects_http_curl_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tools.yaml");
        fs::write(
            &path,
            r#"
version: 1
shell:
  allowed_programs: [curl]
  default_timeout_ms: 1000
  max_timeout_ms: 2000
  max_output_bytes: 4096
curl:
  url_prefixes: ["http://evil.example/"]
"#,
        )
        .unwrap();
        let err = load_tools_file(&path).unwrap_err();
        assert!(err.to_string().contains("https://"));
    }

    #[test]
    fn loads_repo_tools_yaml_if_present() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(DEFAULT_TOOLS_CONFIG);
        if !path.is_file() {
            return;
        }
        let file = load_tools_file(&path).unwrap();
        assert_eq!(file.version, Some(1));
        assert!(file.shell.is_set());
    }
}
