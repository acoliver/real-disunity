//! Errors of the GPU-facing operations.

use crate::layout::LayoutError;
use crate::module::ModuleError;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProgramError {
    #[error(transparent)]
    Module(#[from] ModuleError),
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error("the {kind} handle is not a live resource of this runtime")]
    UnknownHandle { kind: &'static str },
    #[error("set {set} binding {binding} has no resource")]
    MissingBinding { set: u32, binding: u32 },
    #[error("set {set} binding {binding} is not a binding of the program, or is bound twice")]
    UnexpectedBinding { set: u32, binding: u32 },
    #[error("set {set} binding {binding} needs {expected} but got {found}")]
    WrongResource {
        set: u32,
        binding: u32,
        expected: String,
        found: String,
    },
    #[error("set {set} binding {binding} needs {needed} bytes and the buffer has {have}")]
    BufferTooSmall {
        set: u32,
        binding: u32,
        needed: u64,
        have: u64,
    },
    #[error(
        "set {set} binding {binding} is a comparison sampler, which this renderer does not create"
    )]
    UnsupportedSampler { set: u32, binding: u32 },
    #[error("the program uses bind group {set}, above the device's limit of {limit}")]
    TooManyGroups { set: u32, limit: u32 },
    #[error("the target has no depth attachment but a depth clear or test was requested")]
    NoDepth,
    #[error("the pixel data is {given} bytes and the {size:?} {format:?} texture needs {needed}")]
    TextureData {
        size: [u32; 2],
        format: wgpu::TextureFormat,
        needed: usize,
        given: usize,
    },
    #[error("writing {len} bytes at {offset} does not fit a buffer of {size}")]
    WriteRange { offset: u64, len: u64, size: u64 },
    #[error("the program has a {found:?} stage where a {expected:?} stage is needed")]
    WrongStage {
        expected: crate::module::StageKind,
        found: crate::module::StageKind,
    },
    #[error("the GPU rejected the operation: {0}")]
    Gpu(String),
    #[error("reading back from the GPU failed: {0}")]
    Readback(String),
}
