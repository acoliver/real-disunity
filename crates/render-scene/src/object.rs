//! Object materials on the program runtime, slice 2 of issue 450.
//!
//! This commit freezes the fixed forward object path in
//! [`crate::renderer::legacy_objects`] and pins it with byte-equality captures. The scene still
//! draws with the fixed pipelines, so the claim that objects are recorded through the runtime
//! is an ignored test until the runtime path lands.

mod capture_tests;
mod cost_tests;
mod program_tests;
