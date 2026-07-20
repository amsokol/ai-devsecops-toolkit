//! Deterministic preflight: manifests → required programs → allowlist ∩ PATH.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::Path;

use api::depbot::v1::{DoctorIssue, DoctorReport, EcosystemNeed, ToolsFile};

use crate::config::Error as ConfigError;

/// Run doctor against a workspace root and loaded tools config.
pub fn run_doctor(workspace_root: &Path, tools: &ToolsFile) -> Result<DoctorReport, ConfigError> {
    let allowlist = allowlist_from_tools(tools);
    let mut needs = detect_ecosystem_needs(workspace_root)?;

    // BCR lookups use curl when the host allowlists it; do not force curl onto
    // every Bazel repo that omitted it from tools.yaml.
    for need in &mut needs {
        if need.ecosystem.as_deref() != Some("bazel") {
            continue;
        }
        if allowlist.contains("curl")
            && !need.required_programs.iter().any(|p| p.eq_ignore_ascii_case("curl"))
        {
            need.required_programs.push("curl".to_owned());
        }
    }

    let mut issues: Vec<DoctorIssue> = Vec::new();
    let mut seen_issue_keys: BTreeSet<(String, String)> = BTreeSet::new();

    for need in &needs {
        for program in &need.required_programs {
            let prog = program.trim();
            if prog.is_empty() {
                continue;
            }
            let key = (
                prog.to_ascii_lowercase(),
                need.ecosystem.clone().unwrap_or_default(),
            );
            if !seen_issue_keys.insert(key) {
                continue;
            }

            let in_allowlist = allowlist.contains(&prog.to_ascii_lowercase());
            let on_path = program_on_path(prog);

            if !in_allowlist {
                let ecosystem = need.ecosystem.clone().unwrap_or_default();
                let evidence = need.evidence_path.as_deref().unwrap_or("?");
                issues.push(
                    DoctorIssue::default()
                        .with_program(prog)
                        .with_ecosystem(&ecosystem)
                        .with_message(format!(
                            "for {ecosystem} dependencies (found `{evidence}`) program `{prog}` \
                             is required but not listed in `.depbot/tools.yaml` allowed_programs; \
                             cannot continue"
                        )),
                );
                continue;
            }

            if !on_path {
                let ecosystem = need.ecosystem.clone().unwrap_or_default();
                let evidence = need.evidence_path.as_deref().unwrap_or("?");
                issues.push(
                    DoctorIssue::default()
                        .with_program(prog)
                        .with_ecosystem(&ecosystem)
                        .with_message(format!(
                            "for {ecosystem} dependencies (found `{evidence}`) `{prog}` must be \
                             installed and on PATH, but it was not found; cannot continue"
                        )),
                );
            }
        }
    }

    let ok = issues.is_empty();
    let mut report = DoctorReport::default().with_ok(ok);
    report.needs = needs;
    report.issues = issues;
    Ok(report)
}

/// Format a failed [`DoctorReport`] for CLI stderr / exit error.
pub fn format_doctor_failure(report: &DoctorReport) -> String {
    let mut lines = vec![
        "depbot doctor failed: required tools are missing or not allowlisted.".to_owned(),
    ];
    for issue in &report.issues {
        let msg = issue.message.as_deref().unwrap_or("unknown issue");
        lines.push(format!("- {msg}"));
    }
    if !report.needs.is_empty() {
        lines.push("Detected ecosystems:".to_owned());
        for need in &report.needs {
            let eco = need.ecosystem.as_deref().unwrap_or("?");
            let ev = need.evidence_path.as_deref().unwrap_or("?");
            let progs = need.required_programs.join(", ");
            lines.push(format!("- {eco} ({ev}) → requires: {progs}"));
        }
    }
    lines.join("\n")
}

/// Format a successful [`DoctorReport`] for the agent system prompt.
///
/// Gives the model deterministic ecosystem facts so it does not re-probe
/// absent manifests (e.g. `go.mod` / `package.json`) on every turn.
pub fn format_doctor_context(report: &DoctorReport) -> String {
    let mut lines = vec![
        "## Depbot doctor (preflight — trust these facts)".to_owned(),
        String::new(),
        "A deterministic preflight already scanned the **workspace root** for ecosystem \
         manifests and verified required programs are allowlisted and on PATH."
            .to_owned(),
        "Do **not** spend tool calls confirming missing root manifests that are absent below \
         (e.g. do not `read_file` `go.mod` / `package.json` unless a nested path is in scope)."
            .to_owned(),
        "You may still open the listed evidence files when you need their contents for scanning \
         or comments."
            .to_owned(),
        String::new(),
    ];

    if report.needs.is_empty() {
        lines.push(
            "**Detected ecosystems (root):** none — no `go.mod`, `package.json`, `Cargo.toml`, \
             `MODULE.bazel` / `*.MODULE.bazel`, or `buf.yaml` at the workspace root."
                .to_owned(),
        );
    } else {
        lines.push("**Detected ecosystems (root):**".to_owned());
        for need in &report.needs {
            let eco = need.ecosystem.as_deref().unwrap_or("?");
            let ev = need.evidence_path.as_deref().unwrap_or("?");
            let progs = need.required_programs.join(", ");
            lines.push(format!(
                "- `{eco}` — evidence `{ev}`; tools ready: {progs}"
            ));
        }
    }

    lines.push(String::new());
    lines.push(
        "**Not detected at workspace root** (skip probing unless the user asks about nested \
         trees): any ecosystem missing from the list above among go / npm / cargo / bazel / buf."
            .to_owned(),
    );

    lines.join("\n")
}

fn allowlist_from_tools(tools: &ToolsFile) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(shell) = tools.shell.as_option() {
        for p in &shell.allowed_programs {
            let t = p.trim();
            if !t.is_empty() {
                out.insert(t.to_ascii_lowercase());
            }
        }
    }
    out
}

fn detect_ecosystem_needs(workspace_root: &Path) -> Result<Vec<EcosystemNeed>, ConfigError> {
    let mut by_eco: BTreeMap<String, EcosystemNeed> = BTreeMap::new();

    let go_mod = workspace_root.join("go.mod");
    if go_mod.is_file() {
        insert_need(&mut by_eco, "go", "go.mod", &["go"]);
    }

    let package_json = workspace_root.join("package.json");
    if package_json.is_file() {
        let pm = if workspace_root.join("pnpm-lock.yaml").is_file() {
            "pnpm"
        } else if workspace_root.join("yarn.lock").is_file() {
            "yarn"
        } else {
            "npm"
        };
        insert_need(&mut by_eco, "npm", "package.json", &[pm]);
    }

    let cargo_toml = workspace_root.join("Cargo.toml");
    if cargo_toml.is_file() {
        insert_need(&mut by_eco, "cargo", "Cargo.toml", &["cargo"]);
    }

    let module_bazel = workspace_root.join("MODULE.bazel");
    let has_rust_module = workspace_has_rust_module_bazel(workspace_root);
    if module_bazel.is_file() || has_rust_module {
        let evidence = if module_bazel.is_file() {
            "MODULE.bazel"
        } else {
            "rust.MODULE.bazel"
        };
        insert_need(&mut by_eco, "bazel", evidence, &["bazel"]);
    }

    let buf_yaml = workspace_root.join("buf.yaml");
    if buf_yaml.is_file() {
        insert_need(&mut by_eco, "buf", "buf.yaml", &["buf"]);
    }

    Ok(by_eco.into_values().collect())
}

fn workspace_has_rust_module_bazel(workspace_root: &Path) -> bool {
    let direct = workspace_root.join("rust.MODULE.bazel");
    if direct.is_file() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(workspace_root) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.ends_with(".MODULE.bazel") && entry.path().is_file() {
            return true;
        }
    }
    false
}

fn insert_need(
    map: &mut BTreeMap<String, EcosystemNeed>,
    ecosystem: &str,
    evidence: &str,
    programs: &[&str],
) {
    let entry = map.entry(ecosystem.to_owned()).or_insert_with(|| {
        EcosystemNeed::default()
            .with_ecosystem(ecosystem)
            .with_evidence_path(evidence)
    });
    for p in programs {
        let s = (*p).to_owned();
        if !entry.required_programs.iter().any(|x| x == &s) {
            entry.required_programs.push(s);
        }
    }
}

/// True if `program` resolves on PATH (Windows: respects PATHEXT).
pub fn program_on_path(program: &str) -> bool {
    let program = program.trim();
    if program.is_empty() {
        return false;
    }
    let as_path = Path::new(program);
    if as_path.components().count() > 1 || as_path.is_absolute() {
        return as_path.is_file();
    }

    let Some(path_os) = env::var_os("PATH") else {
        return false;
    };

    let exts = path_extensions();
    for dir in env::split_paths(&path_os) {
        if candidate_exists(&dir, program, &exts) {
            return true;
        }
    }
    false
}

fn path_extensions() -> Vec<String> {
    #[cfg(windows)]
    {
        match env::var_os("PATHEXT") {
            Some(v) => env::split_paths(&v)
                .filter_map(|p| p.to_str().map(|s| s.to_ascii_lowercase()))
                .collect(),
            None => vec![".exe".into(), ".cmd".into(), ".bat".into(), ".com".into()],
        }
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

fn candidate_exists(dir: &Path, program: &str, exts: &[String]) -> bool {
    let direct = dir.join(program);
    if direct.is_file() {
        return true;
    }
    for ext in exts {
        let mut name = program.to_owned();
        if !name.to_ascii_lowercase().ends_with(ext) {
            name.push_str(ext);
        }
        if dir.join(&name).is_file() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use api::depbot::v1::ShellSection;
    use std::fs;

    fn tools_with(programs: &[&str]) -> ToolsFile {
        use buffa_types::google::protobuf::Duration as ProtoDuration;
        use std::time::Duration;

        let mut shell = ShellSection::default().with_max_output_bytes(4096);
        shell.default_timeout = ProtoDuration::from(Duration::from_secs(1)).into();
        shell.max_timeout = ProtoDuration::from(Duration::from_secs(2)).into();
        shell.allowed_programs = programs.iter().map(|s| (*s).to_owned()).collect();
        let mut file = ToolsFile::default().with_version(1);
        file.shell = shell.into();
        file
    }

    #[test]
    fn detects_cargo_and_requires_cargo() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let tools = tools_with(&["cargo"]);
        let report = run_doctor(dir.path(), &tools).unwrap();
        assert!(
            report.needs.iter().any(|n| n.ecosystem.as_deref() == Some("cargo"))
        );
        assert_eq!(
            report.issues.iter().any(|i| i.program.as_deref() == Some("cargo")),
            !program_on_path("cargo")
        );
    }

    #[test]
    fn fails_when_cargo_not_allowlisted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let tools = tools_with(&["git"]);
        let report = run_doctor(dir.path(), &tools).unwrap();
        assert_eq!(report.ok, Some(false));
        assert!(report.issues.iter().any(|i| i.program.as_deref() == Some("cargo")));
        let msg = format_doctor_failure(&report);
        assert!(msg.contains("allowed_programs"), "{msg}");
    }

    #[test]
    fn nonsense_program_not_on_path() {
        assert!(!program_on_path("go-definitely-not-installed-xyz-depbot"));
    }

    #[test]
    fn npm_picks_pnpm_from_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{}\n").unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "lockfileVersion: 9\n").unwrap();
        let tools = tools_with(&["pnpm"]);
        let report = run_doctor(dir.path(), &tools).unwrap();
        let npm = report
            .needs
            .iter()
            .find(|n| n.ecosystem.as_deref() == Some("npm"))
            .unwrap();
        assert_eq!(npm.required_programs, vec!["pnpm".to_owned()]);
    }

    #[test]
    fn bazel_adds_curl_only_when_allowlisted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("MODULE.bazel"), "module(name = \"x\")\n").unwrap();

        let without_curl = tools_with(&["bazel"]);
        let report = run_doctor(dir.path(), &without_curl).unwrap();
        let bazel = report
            .needs
            .iter()
            .find(|n| n.ecosystem.as_deref() == Some("bazel"))
            .unwrap();
        assert_eq!(bazel.required_programs, vec!["bazel".to_owned()]);

        let with_curl = tools_with(&["bazel", "curl"]);
        let report = run_doctor(dir.path(), &with_curl).unwrap();
        let bazel = report
            .needs
            .iter()
            .find(|n| n.ecosystem.as_deref() == Some("bazel"))
            .unwrap();
        assert_eq!(
            bazel.required_programs,
            vec!["bazel".to_owned(), "curl".to_owned()]
        );
    }

    #[test]
    fn doctor_context_lists_detected_and_skips_absent() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let tools = tools_with(&["cargo"]);
        let report = run_doctor(dir.path(), &tools).unwrap();
        let ctx = format_doctor_context(&report);
        assert!(ctx.contains("`cargo`"), "{ctx}");
        assert!(ctx.contains("evidence `Cargo.toml`"), "{ctx}");
        assert!(ctx.contains("Do **not** spend tool calls"), "{ctx}");
        assert!(ctx.contains("Not detected at workspace root"), "{ctx}");
    }
}
