//! Errors of the GPU-facing operations.

use worldrunner_render_gpu::submission::{LeaseError, SubmitError, TransactionError};
use worldrunner_render_gpu::texture::UploadError;

use crate::constants::ConstantError;
use crate::layout::LayoutError;
use crate::module::{ModuleError, OverrideError};
use crate::variant::VariantError;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProgramError {
    #[error(transparent)]
    Module(#[from] ModuleError),
    #[error(transparent)]
    Override(#[from] OverrideError),
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    Constants(#[from] ConstantError),
    #[error(transparent)]
    Variant(#[from] VariantError),
    #[error("program {program} has no variants")]
    NoVariants { program: String },
    #[error("program {program} has no property {property}")]
    UnknownProperty { program: String, property: String },
    #[error("property {property} takes {expected} and was given {found}")]
    WrongPropertyKind {
        property: String,
        expected: String,
        found: String,
    },
    #[error("program {program} uses property {property} as two different kinds of value")]
    PropertyConflict { program: String, property: String },
    #[error("property {property} has no value for set {set} binding {binding}")]
    MissingProperty {
        property: String,
        set: u32,
        binding: u32,
    },
    #[error("the state describes {states} colour targets and the pass has {targets}")]
    ColorStates { states: usize, targets: usize },
    #[error(
        "the pass has a depth attachment and no colour attachment, and the state describes {states} colour targets"
    )]
    ColorStatesOnDepthOnly { states: usize },
    #[error("the program has no fragment stage, and the pass has {colors} colour attachments")]
    NoFragmentForColors { colors: usize },
    #[error(
        "set {set} binding {binding} imports {semantic}, and the draw imports nothing of that name"
    )]
    MissingImport {
        semantic: String,
        set: u32,
        binding: u32,
    },
    #[error("the draw imports {semantic}, which no binding of the program takes")]
    UnusedImport { semantic: String },
    #[error("the draw imports {semantic} more than once")]
    DuplicateImport { semantic: String },
    #[error(
        "set {set} is lent by the pass owner, and the draw or the program gives it resources too"
    )]
    ExternalSetConflict { set: u32 },
    #[error("the program has no set {set} that the runtime builds a group for")]
    UnknownSet { set: u32 },
    #[error(
        "the prepared set for set {set} was built for another layout or runtime than the program has there"
    )]
    SetLayoutMismatch { set: u32 },
    #[error(
        "set {set} is given as a prepared set more than once, or alongside resources of its own"
    )]
    PreparedSetConflict { set: u32 },
    #[error("the geometry was made for another vertex layout than the pipeline reads")]
    GeometryLayoutMismatch,
    #[error("the vertex layout has {expected} buffers and the geometry was given {found}")]
    VertexBufferCount { expected: usize, found: usize },
    #[error("the instance range {start}..{end} ends before it starts")]
    InstanceRange { start: u32, end: u32 },
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
        "set {set} binding {binding} is bound to a buffer range at offset {offset}, which is not a multiple of the device's offset alignment {alignment}"
    )]
    BufferRangeMisaligned {
        set: u32,
        binding: u32,
        offset: u64,
        alignment: u64,
    },
    #[error(
        "set {set} binding {binding} is bound to {size} bytes at offset {offset} of a buffer of {len} bytes"
    )]
    BufferRangePastBuffer {
        set: u32,
        binding: u32,
        offset: u64,
        size: u64,
        len: u64,
    },
    #[error(
        "set {set} binding {binding} is a storage image the bound texture does not suit: {error}"
    )]
    StorageImage {
        set: u32,
        binding: u32,
        error: StorageImageError,
    },
    #[error(
        "the program binds {count} storage images in its {stage:?} stage and the device allows {limit}"
    )]
    StorageImageLimit {
        stage: crate::module::StageKind,
        count: u32,
        limit: u32,
    },
    #[error("the device cannot bind {format:?} as a storage image with {access:?} access")]
    UnsupportedStorageFormat {
        format: wgpu::TextureFormat,
        access: wgpu::StorageTextureAccess,
    },
    #[error(
        "set {set} binding {binding} and set {other_set} binding {other_binding} bind one texture \
         and at least one of them writes it, which one pass cannot do"
    )]
    AliasedTexture {
        set: u32,
        binding: u32,
        other_set: u32,
        other_binding: u32,
    },
    #[error(
        "set {set} binding {binding} is a comparison sampler, which this renderer does not create"
    )]
    UnsupportedSampler { set: u32, binding: u32 },
    #[error("the program uses bind group {set}, above the device's limit of {limit}")]
    TooManyGroups { set: u32, limit: u32 },
    #[error(
        "the state asks for alpha to coverage, which needs a pass of more than one sample per pixel, and the pass has {samples}"
    )]
    AlphaToCoverageNeedsSamples { samples: u32 },
    #[error("the state asks for {what}, which this renderer does not draw")]
    UnsupportedState { what: &'static str },
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
    #[error("the runtime may hold {budget} bytes and the arena reserves {arena}")]
    ArenaTooSmall { budget: u64, arena: u64 },
    #[error(
        "the runtime already holds {live} bytes allocated before any arena, which the arena's reservation would not cover"
    )]
    ArenaAfterAllocations { live: u64 },
    #[error("the arena was opened by another runtime")]
    ForeignArena,
    #[error("the draws were prepared for {prepared:?} and the pass is {pass:?}")]
    TargetMismatch {
        prepared: Box<crate::packet::TargetSignature>,
        pass: Box<crate::packet::TargetSignature>,
    },
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
    UnsupportedTarget(#[from] UnsupportedTarget),
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

/// Why a texture does not suit the storage image a shader declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StorageImageError {
    #[error("the shader declares {declared:?} and the texture is {found:?}")]
    Format {
        declared: wgpu::TextureFormat,
        found: wgpu::TextureFormat,
    },
    #[error("the texture's usage {usage:?} does not include storage binding")]
    Usage { usage: wgpu::TextureUsages },
    #[error(
        "the shader's {declared:?} view needs a texture of another shape than a {found:?} texture of {layers} layers"
    )]
    Dimension {
        declared: wgpu::TextureViewDimension,
        found: wgpu::TextureDimension,
        layers: u32,
    },
    #[error("the texture has {samples} samples per texel and a storage image has one")]
    Multisampled { samples: u32 },
    #[error("the device cannot bind {format:?} as a storage image with {access:?} access")]
    Unsupported {
        format: wgpu::TextureFormat,
        access: wgpu::StorageTextureAccess,
    },
}

/// A pass the runtime does not draw into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UnsupportedTarget {
    #[error("the pass has {samples} samples per pixel; program draws are single-sample")]
    Multisampled { samples: u32 },
    #[error("the pass has {samples} samples per pixel; draws support 1, 2, 4, 8 or 16")]
    SampleCount { samples: u32 },
    #[error("the pass has {count} color attachments, which the signature constructor refuses")]
    ColorAttachments { count: usize },
    #[error("{format:?} has a stencil aspect; program draws do not use stencil")]
    Stencil { format: wgpu::TextureFormat },
    #[error("{format:?} has no depth aspect")]
    NotDepth { format: wgpu::TextureFormat },
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
