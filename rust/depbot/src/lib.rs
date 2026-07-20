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

pub use config::{
    load_tools_file, shell_tool_config_from_tools_file, Error as ConfigError, DEFAULT_TOOLS_CONFIG,
};

pub use api::depbot::v1::{CurlSection, ShellSection, ToolsFile};
