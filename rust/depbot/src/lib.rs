//! `depbot` — product-side helpers for the dependency agent.
//!
//! On-disk tools config is `depbot.v1.ToolsFile` (YAML → JSON → protobuf).

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]

mod config;
mod doctor;

pub use config::{
    load_tools_file, shell_tool_config_from_tools_file, Error as ConfigError, DEFAULT_TOOLS_CONFIG,
};
pub use doctor::{format_doctor_context, format_doctor_failure, program_on_path, run_doctor};

pub use api::depbot::v1::{
    CurlSection, DoctorIssue, DoctorReport, EcosystemNeed, ShellSection, ToolsFile,
};
