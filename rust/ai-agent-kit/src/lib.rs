//! `ai-agent-kit` — reusable AI agent runtime primitives.
//!
//! Phase 1 exposes [`load_skills`]: load human-readable skill markdown from
//! `.skills/<skill_id>/` into protobuf (`aiagentkit.v1`) types.

mod skills;

pub use skills::{load_skills, Error};

pub use api::aiagentkit::v1::{LoadSkills, SkillBundle, SkillFile};
/// Zero-copy buffa views (for decode-from-bytes paths).
pub use api::aiagentkit::v1::view::{
    LoadSkillsOwnedView, LoadSkillsView, SkillBundleOwnedView, SkillBundleView,
    SkillFileOwnedView, SkillFileView,
};
