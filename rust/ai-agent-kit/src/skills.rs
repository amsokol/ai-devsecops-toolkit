//! Load agent skills from `.skills/<skill_id>/` in a workspace.

use std::fs;
use std::path::{Path, PathBuf};

use api::aiagentkit::v1::{LoadSkills, SkillBundle, SkillFile};

/// Errors from [`load_skills`].
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("workspace_root is required")]
    MissingWorkspaceRoot,

    #[error("skill_id is required")]
    MissingSkillId,

    #[error("skill directory not found: {0}")]
    SkillDirNotFound(PathBuf),

    #[error("skill path is not a directory: {0}")]
    SkillPathNotDir(PathBuf),

    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Load markdown skills from `<workspace_root>/.skills/<skill_id>/`.
///
/// Call sites pass [`LoadSkills`] and receive [`SkillBundle`]. Protobuf
/// `(buf.validate.*)` annotations document the schema; runtime protovalidate
/// can be wired later when a buffa-compatible validator matches our stack.
pub fn load_skills(params: &LoadSkills) -> Result<SkillBundle, Error> {
    let workspace_root = params
        .workspace_root
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(Error::MissingWorkspaceRoot)?;

    let skill_id = params
        .skill_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(Error::MissingSkillId)?;

    let skill_dir = Path::new(workspace_root).join(".skills").join(skill_id);

    if !skill_dir.exists() {
        return Err(Error::SkillDirNotFound(skill_dir));
    }
    if !skill_dir.is_dir() {
        return Err(Error::SkillPathNotDir(skill_dir));
    }

    let mut names: Vec<String> = Vec::new();
    let entries = fs::read_dir(&skill_dir).map_err(|source| Error::Io {
        path: skill_dir.clone(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| Error::Io {
            path: skill_dir.clone(),
            source,
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.ends_with(".md") {
            names.push(name.to_owned());
        }
    }
    names.sort();

    let mut files: Vec<SkillFile> = Vec::with_capacity(names.len());
    for name in names {
        let path = skill_dir.join(&name);
        let content = fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        files.push(SkillFile::default().with_name(name).with_content(content));
    }

    let root_path = skill_dir
        .canonicalize()
        .unwrap_or(skill_dir)
        .to_string_lossy()
        .into_owned();

    let mut bundle = SkillBundle::default()
        .with_skill_id(skill_id.to_owned())
        .with_root_path(root_path);
    bundle.files = files;
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn params(workspace_root: impl Into<String>, skill_id: impl Into<String>) -> LoadSkills {
        LoadSkills::default()
            .with_workspace_root(workspace_root)
            .with_skill_id(skill_id)
    }

    #[test]
    fn loads_markdown_skills_sorted() {
        let dir = tempdir().unwrap();
        let skill = dir.path().join(".skills").join("depbot");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# Depbot\n").unwrap();
        fs::write(skill.join("grouping.md"), "## Grouping\n").unwrap();
        fs::write(skill.join("notes.txt"), "ignored").unwrap();

        let bundle = load_skills(&params(dir.path().to_string_lossy(), "depbot")).unwrap();

        assert_eq!(bundle.skill_id.as_deref(), Some("depbot"));
        assert_eq!(bundle.files.len(), 2);
        assert_eq!(bundle.files[0].name.as_deref(), Some("SKILL.md"));
        assert_eq!(bundle.files[0].content.as_deref(), Some("# Depbot\n"));
        assert_eq!(bundle.files[1].name.as_deref(), Some("grouping.md"));
    }

    #[test]
    fn missing_skill_dir_errors() {
        let dir = tempdir().unwrap();
        let err = load_skills(&params(dir.path().to_string_lossy(), "missing")).unwrap_err();
        assert!(matches!(err, Error::SkillDirNotFound(_)));
    }

    #[test]
    fn empty_skill_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_skills(&params(dir.path().to_string_lossy(), "  ")).unwrap_err();
        assert!(matches!(err, Error::MissingSkillId));
    }
}
