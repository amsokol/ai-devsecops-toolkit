//! Checked-in Protobuf (buffa) stubs for `aiagentkit.v1` and `depbot.v1`.
//!
//! Generated hermetically via:
//!   bazel run //api:generate
//! (Buf CLI + BSR remote `buf.build/anthropics/buffa`).
//!
//! Package layout under `api/` is split so `ai-agent-kit` can move to its own
//! repo without taking `depbot` protos. Call sites:
//! `api::aiagentkit::v1::{...}`, `api::depbot::v1::{...}`.

#![allow(clippy::all)]
#![allow(
    dead_code,
    unused_imports,
    unused_qualifications,
    non_camel_case_types
)]

/// Buffa-generated message types.
pub mod buffa {
    pub mod aiagentkit {
        pub mod v1 {
            include!("../gen/buffa/aiagentkit.v1.rs");
        }
    }
    pub mod depbot {
        pub mod v1 {
            include!("../gen/buffa/depbot.v1.rs");
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

/// Facade matching the proto package path: `api::depbot::v1`.
pub mod depbot {
    pub mod v1 {
        pub use crate::buffa::depbot::v1::*;

        /// Zero-copy message views.
        pub mod view {
            pub use crate::buffa::depbot::v1::__buffa::view::*;
        }
    }
}
