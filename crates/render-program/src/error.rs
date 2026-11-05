//! Errors of the GPU-facing operations.

use worldrunner_render_gpu::submission::{LeaseError, SubmitError, TransactionError};
use worldrunner_render_gpu::texture::UploadError;

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
    #[error("a draw of {count} starting at {first} runs past the largest 32-bit index")]
    DrawRangeOverflow { first: u32, count: u32 },
    #[error("vertex buffer {slot} holds {have} bytes and the draw reads up to byte {needed} of it")]
    VertexRangePastBuffer { slot: usize, needed: u64, have: u64 },
    #[error("the index buffer holds {available} indices and the draw reads up to index {end}")]
    IndexRangePastBuffer { end: u64, available: u64 },
    #[error(
        "base vertex {base_vertex} starts every index past the {vertices} vertices vertex buffer {slot} holds"
    )]
    BaseVertexPastBuffer {
        slot: usize,
        base_vertex: i32,
        vertices: u64,
    },
    #[error("the allocation takes the runtime to {required} bytes, over its budget of {budget}")]
    OverBudget { required: u64, budget: u64 },
    #[error("the allocation is invalid: {0}")]
    InvalidAllocation(&'static str),
    #[error("writing {len} bytes at {offset} is not a multiple of four")]
    WriteAlignment { offset: u64, len: u64 },
    #[error("the program has a {found:?} stage where a {expected:?} stage is needed")]
    WrongStage {
        expected: crate::module::StageKind,
        found: crate::module::StageKind,
    },
    #[error(transparent)]
    TextureUpload(#[from] UploadError),
    /// The encoder is not one the submission authority handed out for the frame being recorded.
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error(transparent)]
    Lease(#[from] LeaseError),
    #[error("an unsubmitted frame is already prepared")]
    Busy,
    #[error("device lost: {0}")]
    DeviceLost(String),
    #[error("the GPU rejected the operation: {0}")]
    Gpu(String),
    #[error("reading back from the GPU failed: {0}")]
    Readback(String),
}

/// The authority's refusals keep their names here, so a caller sees device loss and a busy
/// queue the same way whether it came from beginning a frame or from using the runtime.
impl From<SubmitError> for ProgramError {
    fn from(error: SubmitError) -> Self {
        match error {
            SubmitError::Lease(error) => Self::Lease(error),
            SubmitError::Transaction(error) => Self::Transaction(error),
            SubmitError::Busy => Self::Busy,
            SubmitError::DeviceLost(message) => Self::DeviceLost(message),
        }
    }
}
