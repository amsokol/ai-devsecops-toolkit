//! Checked-in Protobuf (buffa) stubs for `aiagentkit.v1`.
//!
//! Generated hermetically via:
//!   bazel run //api/ai-agent-kit/v1:generate
//! (Buf CLI + BSR remote `buf.build/anthropics/buffa`).
//!
//! Call sites: `api::aiagentkit::v1::{...}`.

#![allow(clippy::all)]
#![allow(
    dead_code,
    unused_imports,
    unused_qualifications,
    non_camel_case_types
)]

/// Buffa-generated message types (`crate::buffa::aiagentkit::v1`).
pub mod buffa {
    pub mod aiagentkit {
        pub mod v1 {
            include!("../gen/buffa/aiagentkit.v1.rs");
        }
    }
}

/// Facade matching the proto package path: `api::aiagentkit::v1`.
pub mod aiagentkit {
    pub mod v1 {
        pub use crate::buffa::aiagentkit::v1::*;

        /// Zero-copy message views (`SkillBundleView`, `SkillBundleOwnedView`, …).
        pub mod view {
            pub use crate::buffa::aiagentkit::v1::__buffa::view::*;
        }
    }
}
