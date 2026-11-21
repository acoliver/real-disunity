//! What a feature's preparation stages into the frame's encoder: buffer and texture copies and
//! compute dispatches, encoded in staging order once every preparation has succeeded.
use super::{
    AttachmentView, FrameError, PassDeclaration, PassEntry, PreparedCommands, ResourceRef,
};

/// One operation staged by a preparation, encoded in staging order.
pub(super) enum Staged {
    Copy {
        source: (wgpu::Buffer, u64),
        destination: (wgpu::Buffer, u64),
        bytes: u64,
    },
    TextureCopy {
        source: wgpu::TexelCopyTextureInfoBase<wgpu::Texture>,
        destination: wgpu::TexelCopyTextureInfoBase<wgpu::Texture>,
        size: wgpu::Extent3d,
    },
    Dispatch {
        label: &'static str,
        pipeline: wgpu::ComputePipeline,
        group: wgpu::BindGroup,
        workgroups: [u32; 3],
    },
    Prepared {
        label: &'static str,
        commands: Box<dyn PreparedCommands>,
    },
}
fn owned(info: wgpu::TexelCopyTextureInfo<'_>) -> wgpu::TexelCopyTextureInfoBase<wgpu::Texture> {
    wgpu::TexelCopyTextureInfoBase {
        texture: info.texture.clone(),
        mip_level: info.mip_level,
        origin: info.origin,
        aspect: info.aspect,
    }
}

pub(super) fn borrowed(
    info: &wgpu::TexelCopyTextureInfoBase<wgpu::Texture>,
) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture: &info.texture,
        mip_level: info.mip_level,
        origin: info.origin,
        aspect: info.aspect,
    }
}

pub struct PrepareContext<'a> {
    device: &'a wgpu::Device,
    pub(super) encoder: &'a mut wgpu::CommandEncoder,
    publications: worldrunner_render_gpu::submission::Publications,
    pub(super) staged: Vec<Staged>,
    pub(super) views: Vec<AttachmentView>,
    pub(super) passes: Vec<PassDeclaration>,
    pub(super) completed: Vec<PassEntry>,
    /// Whether features are preparing, the only time prepared commands may be staged.
    pub(super) preparing: bool,
    /// The views the staged prepared commands write, in staging order.
    pub(super) prepared_written: Vec<ResourceRef>,
    /// The compute passes the frame records and the work staged for each.
    pub(super) compute: super::ComputeStaging,
}
impl<'a> PrepareContext<'a> {
    pub fn new(
        device: &'a wgpu::Device,
        encoder: &'a mut wgpu::CommandEncoder,
    ) -> Result<Self, worldrunner_render_gpu::submission::TransactionError> {
        let publications = worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?;
        Ok(Self {
            device,
            encoder,
            publications,
            staged: Vec::new(),
            views: Vec::new(),
            passes: Vec::new(),
            completed: Vec::new(),
            preparing: false,
            prepared_written: Vec::new(),
            compute: super::ComputeStaging::default(),
        })
    }
    /// Whether a validated geometry prefix already encoded this entry in the same frame.
    pub fn completed_in_prefix(&self, entry: PassEntry) -> bool {
        self.completed.contains(&entry)
    }
    pub(crate) fn set_completed_prefix(&mut self, completed: &[PassEntry]) {
        self.completed = completed.to_vec();
    }
    pub fn publications(&self) -> worldrunner_render_gpu::submission::Publications {
        self.publications.clone()
    }
    /// Stages an infallible resident edit in the active prepared frame.
    pub fn publish(&mut self, publish: impl FnOnce() + 'static) {
        self.publications.stage(publish);
    }
    pub(crate) fn set_views(&mut self, views: &[AttachmentView]) {
        self.views = views.to_vec();
    }
    pub(crate) fn set_passes(&mut self, passes: &[PassDeclaration]) {
        self.passes = passes.to_vec();
    }
    /// Frame-local checked inputs, available only after full plan validation.
    pub fn resource(&self, resource: ResourceRef) -> Result<&AttachmentView, FrameError> {
        self.views
            .iter()
            .find(|view| view.resource() == resource)
            .ok_or(FrameError::AttachmentView(resource))
    }
    pub fn device(&self) -> &wgpu::Device {
        self.device
    }
    pub fn copy_buffer(&mut self, source: &wgpu::Buffer, destination: &wgpu::Buffer, bytes: u64) {
        self.copy_buffer_region((source, 0), (destination, 0), bytes);
    }
    /// Stages a copy of `bytes` bytes between buffer offsets, encoded after the copies staged
    /// before it. A gather that reads bytes uploaded in this context therefore sees them.
    /// Offsets and `bytes` must be multiples of `wgpu::COPY_BUFFER_ALIGNMENT`.
    pub fn copy_buffer_region(
        &mut self,
        source: (&wgpu::Buffer, u64),
        destination: (&wgpu::Buffer, u64),
        bytes: u64,
    ) {
        self.staged.push(Staged::Copy {
            source: (source.0.clone(), source.1),
            destination: (destination.0.clone(), destination.1),
            bytes,
        });
    }
    /// Stages one compute dispatch of `pipeline` with `group` at set 0, encoded after the copies
    /// and dispatches staged before it, so it reads bytes uploaded or copied earlier in this
    /// context. It shares a compute pass with the compute work staged just before and after it,
    /// inside a debug group of `label`, so `pipeline` must use no other group. A copy staged
    /// between two dispatches ends the pass, so the second dispatch starts a new one.
    ///
    /// The frame checks and holds nothing of it: use it for a pipeline and bind group the feature
    /// owns over its own buffers and textures. Work that writes or reads views of the frame, or
    /// whose resources the frame must hold until it completes, is staged with
    /// [`Self::record_prepared`].
    pub fn dispatch(
        &mut self,
        label: &'static str,
        pipeline: &wgpu::ComputePipeline,
        group: &wgpu::BindGroup,
        workgroups: [u32; 3],
    ) {
        self.staged.push(Staged::Dispatch {
            label,
            pipeline: pipeline.clone(),
            group: group.clone(),
            workgroups,
        });
    }
    /// Stages a copy of `size` texels from `source` to `destination`, encoded after the copies
    /// and dispatches staged before it, so a dispatch staged earlier has written what it copies
    /// and a dispatch staged later reads what it copies. The textures need `COPY_SRC` and
    /// `COPY_DST`, and the regions the copy rules of the format (whole blocks when compressed).
    /// A cancelled or failed frame never encodes it.
    pub fn copy_texture(
        &mut self,
        source: wgpu::TexelCopyTextureInfo<'_>,
        destination: wgpu::TexelCopyTextureInfo<'_>,
        size: wgpu::Extent3d,
    ) {
        self.staged.push(Staged::TextureCopy {
            source: owned(source),
            destination: owned(destination),
            size,
        });
    }
    /// Stages `bytes` for `destination` at `offset`, copied in staging order like
    /// [`Self::copy_buffer_region`]. The bytes are written into the frame authority's reusable
    /// staging now; a cancelled frame never copies them. `bytes` must be a multiple of
    /// `wgpu::COPY_BUFFER_ALIGNMENT` long and fit `destination`, which must allow `COPY_DST`.
    /// An empty upload copies nothing and stages nothing.
    pub fn upload_buffer(&mut self, destination: &wgpu::Buffer, offset: u64, bytes: &[u8]) {
        let size = bytes.len() as u64;
        assert!(
            destination.usage().contains(wgpu::BufferUsages::COPY_DST)
                && offset.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
                && offset
                    .checked_add(size)
                    .is_some_and(|end| end <= destination.size()),
            "upload of {size} bytes at {offset} does not fit {destination:?}"
        );
        if bytes.is_empty() {
            return;
        }
        let source = self.publications.stage_upload(self.device, bytes);
        self.staged.push(Staged::Copy {
            source,
            destination: (destination.clone(), offset),
            bytes: size,
        });
    }
}
