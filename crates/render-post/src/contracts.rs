//! Shared physical resources and authored pass access contracts.
mod geometry;
pub use geometry::{DrawGeometry, IndexBinding};
mod draws;
mod layout_limits;
mod leases;
pub use draws::CheckedDraw;
mod capture;
pub use capture::CompiledCapture;
mod async_readback;
mod bindings;
mod samplers;
pub use samplers::{CheckedSampler, SamplerBinding};
mod buffer_bindings;
mod buffer_scopes;
mod buffers;
mod declared_texture;
mod operation_validation;
mod operations;
mod raster;
mod registry;
pub use declared_texture::DeclaredTexture;
pub(crate) mod stock;
mod transfers;
mod validation;

use std::ops::Range;

pub use async_readback::AsyncReadback;
pub use bindings::{CheckedBindings, TextureBinding, TextureBindingKind};
pub use buffer_bindings::{BufferBinding, BufferBindingKind};
pub use buffers::{BufferUse, BufferUseKind, BufferView};
pub use operations::{CompiledOperations, TypedOperation, TypedWork};
pub use raster::{ColorAttachment, CompiledRaster, DepthAttachment, RasterId, RasterPass};
pub use registry::ResourceRegistry;
pub use transfers::TextureCopy;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AllocationKey {
    id: u64,
    generation: u32,
}
impl AllocationKey {
    pub const fn id(self) -> u64 {
        self.id
    }
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceLifetime {
    ImportedReadOnly,
    PersistentMutable,
    FrameLocal,
}

/// Meaning at an exchange boundary, without prescribing a material layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Semantic {
    Unspecified,
    LinearColor,
    ExposedLinearColor,
    SrgbColor,
    DeviceDepth,
    LinearDepth,
    WorldNormal,
    ViewNormal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureRange {
    pub mips: Range<u32>,
    pub layers: Range<u32>,
    pub aspect: wgpu::TextureAspect,
}
impl TextureRange {
    pub const fn single(mip: u32, layer: u32, aspect: wgpu::TextureAspect) -> Self {
        Self {
            mips: mip..mip.saturating_add(1),
            layers: layer..layer.saturating_add(1),
            aspect,
        }
    }
    pub fn overlaps(&self, other: &Self) -> bool {
        self.mips.start < other.mips.end
            && other.mips.start < self.mips.end
            && self.layers.start < other.layers.end
            && other.layers.start < self.layers.end
            && (self.aspect == other.aspect
                || self.aspect == wgpu::TextureAspect::All
                || other.aspect == wgpu::TextureAspect::All)
    }
    pub(crate) fn contains(&self, other: &Self) -> bool {
        self.mips.start <= other.mips.start
            && self.mips.end >= other.mips.end
            && self.layers.start <= other.layers.start
            && self.layers.end >= other.layers.end
            && (self.aspect == other.aspect || self.aspect == wgpu::TextureAspect::All)
    }
}

/// Constructed from an allocation, never from a caller-supplied view.
#[derive(Clone, Debug)]
pub struct CheckedView {
    allocation: AllocationKey,
    range: TextureRange,
    version: u32,
    view: wgpu::TextureView,
    dimension: wgpu::TextureViewDimension,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    lease: Option<worldrunner_render_gpu::submission::FrameLease>,
}
impl CheckedView {
    pub const fn allocation(&self) -> AllocationKey {
        self.allocation
    }
    pub const fn range(&self) -> &TextureRange {
        &self.range
    }
    pub const fn version(&self) -> u32 {
        self.version
    }
    pub const fn dimension(&self) -> wgpu::TextureViewDimension {
        self.dimension
    }
    /// Selected allocation/alternate format, before native aspect-specific resolution.
    pub const fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
    /// Usages permitted by this view, possibly narrower than its allocation.
    pub const fn usage(&self) -> wgpu::TextureUsages {
        self.usage
    }
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessMode {
    Read,
    Write,
    ReadWrite,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseKind {
    Attachment,
    Sampled,
    Storage,
    Copy,
}

#[derive(Clone, Debug)]
pub struct ResourceUse {
    pub view: CheckedView,
    pub before: Option<u32>,
    pub access: AccessMode,
    pub kind: UseKind,
    pub store: bool,
}
impl ResourceUse {
    pub fn read(view: CheckedView, kind: UseKind) -> Self {
        Self {
            before: Some(view.version),
            view,
            access: AccessMode::Read,
            kind,
            store: true,
        }
    }
    pub fn write(view: CheckedView, kind: UseKind) -> Self {
        Self {
            before: None,
            view,
            access: AccessMode::Write,
            kind,
            store: true,
        }
    }
    pub fn read_write(view: CheckedView, before: u32, kind: UseKind) -> Self {
        Self {
            before: Some(before),
            view,
            access: AccessMode::ReadWrite,
            kind,
            store: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    #[error(transparent)]
    Lease(worldrunner_render_gpu::submission::LeaseError),
    #[error(transparent)]
    CaptureWrite(worldrunner_render_gpu::submission::capture::WriteError),
    #[error(transparent)]
    Transaction(worldrunner_render_gpu::submission::TransactionError),
    #[error("unsupported stock copy source or output contract: {0:?}")]
    CopyOutput(AllocationKey),
    #[error("invalid copy alignment or pitch: {0:?}")]
    CopyAlignment(AllocationKey),
    #[error("invalid copy extent or layout: {0:?}")]
    CopyExtent(AllocationKey),
    #[error("texture buffer copy direction is forbidden (read={read}): {allocation:?}")]
    CopyDirection {
        allocation: AllocationKey,
        read: bool,
    },
    #[error("incompatible whole-buffer usage scope: {0:?}")]
    BufferUsage(AllocationKey),
    #[error("invalid dynamic offset or binding range: {0:?}")]
    DynamicOffset(AllocationKey),
    #[error("invalid compute dispatch: {0:?}")]
    Dispatch([u32; 3]),
    #[error("pipeline or binding layout mismatch")]
    Pipeline,
    #[error("unknown or stale allocation {0:?}")]
    StaleAllocation(AllocationKey),
    #[error("texture creation descriptor does not match native properties")]
    TextureDescriptor,
    #[error("texture creation declaration is unsupported: {0:?}")]
    TextureDeclaration(wgpu::TextureFormat),
    #[error("undeclared texture view format {format:?}: {allocation:?}")]
    ViewFormat {
        allocation: AllocationKey,
        format: wgpu::TextureFormat,
    },
    #[error("invalid subresource range for {0:?}")]
    Range(AllocationKey),
    #[error("conflicting import or replacement of {0:?}")]
    Identity(AllocationKey),
    #[error("read-only allocation written: {0:?}")]
    ReadOnly(AllocationKey),
    #[error("read-only allocation cannot be replaced without initialized contents: {0:?}")]
    ReadOnlyReplacement(AllocationKey),
    #[error("incompatible sampled texture contract: {0:?}")]
    SampleType(AllocationKey),
    #[error("unsupported storage access {access:?} for {format:?}: {allocation:?}")]
    StorageAccess {
        allocation: AllocationKey,
        format: wgpu::TextureFormat,
        access: wgpu::StorageTextureAccess,
    },
    #[error("binding index, visibility or per-stage limit exceeded: {0:?}")]
    BindingLayout(AllocationKey),
    #[error("color attachments require {total} bytes per sample, limit {limit}")]
    ColorAttachmentBytes { total: u32, limit: u32 },
    #[error("backend does not support simultaneous depth attachment sampling: {0:?}")]
    DepthSampling(AllocationKey),
    #[error("missing producer, discarded contents or stale version: {0:?}")]
    Producer(AllocationKey),
    #[error("overlapping incompatible access: {0:?}")]
    Hazard(AllocationKey),
    #[error("incompatible binding, usage or semantic: {0:?}")]
    Binding(AllocationKey),
    #[error("invalid raster attachments or duplicate identity: {0:?}")]
    Raster(RasterId),
    #[error("pass {pass:?}: {source}")]
    Pass { pass: RasterId, source: Box<Self> },
}
