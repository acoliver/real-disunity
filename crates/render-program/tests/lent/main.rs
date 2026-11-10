//! Draws whose bind group sets another owner binds, and the counts that prove what the runtime
//! built. The programs are WGSL stages with no Unity record, and the expected pixels are closed
//! forms of the colours the tests upload.

#[path = "../support/mod.rs"]
mod support;

mod batch;
mod common;
mod comparison;
mod coverage;
mod depth_only;
mod external;
mod overrides;
mod ranges;
mod set_layout;
mod sets;
mod stencil;
mod unfilterable;
