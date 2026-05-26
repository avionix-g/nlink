//! Common test utilities for integration tests.
//!
//! Thin shim over `nlink::lab` so existing integration tests keep
//! working without import changes while the shared helpers live in
//! the public `lab` module.

pub use nlink::lab::LabNamespace as TestNamespace;

/// Check if running as root.
pub fn is_root() -> bool {
    nlink::lab::is_root()
}

/// Skip the test if not running as root.
///
/// Use this at the beginning of integration tests that require root privileges.
///
/// Per Plan 174 — also initializes a `tracing-subscriber` (via
/// [`nlink::lab::init_test_tracing`]) so the lib's
/// `#[tracing::instrument]` spans surface in CI logs. Equivalent to
/// `nlink::require_root!()`; both paths feed through the same
/// subscriber-init helper.
#[macro_export]
macro_rules! require_root {
    () => {
        ::nlink::lab::init_test_tracing();
        if !$crate::common::is_root() {
            eprintln!("Skipping test: requires root");
            return Ok(());
        }
    };
}

/// Skip the test if not running as root (for non-Result functions).
#[macro_export]
macro_rules! require_root_void {
    () => {
        ::nlink::lab::init_test_tracing();
        if !$crate::common::is_root() {
            eprintln!("Skipping test: requires root");
            return;
        }
    };
}
