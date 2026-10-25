use crate::terrain::pages::{
    composition::MaterialImage,
    mip_gpu::{PageInput, PageWriter},
    residency::{PendingPage, ResidentPages},
};
use worldrunner_render_gpu::{configuration::AllocationScope, submission::capture::WriteKey};
use worldrunner_render_post::{
    FrameError, StageId,
    contracts::{
        AllocationKey, RasterId, ResourceLifetime, ResourceRegistry, Semantic, TypedOperation,
        TypedWork,
    },
};

/// Capture segments share physical upload identities. The prefix completes before
/// the suffix reuses these budgeted scratch buffers.
pub(in crate::terrain::pages) struct CaptureUploads {
    registry: ResourceRegistry,
    buffers: Vec<(wgpu::Buffer, AllocationKey)>,
    keys: Vec<WriteKey>,
    next: usize,
}
impl CaptureUploads {
    pub fn new(device: &wgpu::Device, cache: &ResidentPages) -> Result<Self, FrameError> {
        let scope = AllocationScope::new(device);
        let settings = cache.settings();
        let [w, h] = settings.extent();
        let count = settings.parts().upload_bytes_per_frame / settings.page_bytes();
        let mut registry = ResourceRegistry::default();
        let storage = cache.storage();
        let texture = registry.import_leased(
            &storage.texture,
            &storage.owner.lease(),
            Semantic::Unspecified,
            None,
        )?;
        let mut keys = vec![key(texture)];
        let mut buffers = Vec::new();
        for _ in 0..count {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("terrain page capture input"),
                size: u64::from(w) * u64::from(h) * 8,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            let allocation = registry.import_buffer(&buffer, ResourceLifetime::FrameLocal, None)?;
            keys.push(key(allocation));
            buffers.push((buffer, allocation));
        }
        scope.finish().map_err(super::super::preparation_error)?;
        Ok(Self {
            registry,
            buffers,
            keys,
            next: 0,
        })
    }
    pub fn keys(&self) -> impl Iterator<Item = WriteKey> + '_ {
        self.keys.iter().copied()
    }
    pub fn reset(&mut self) {
        self.next = 0;
    }

    pub fn encode(
        &mut self,
        writer: &PageWriter,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        cache: &ResidentPages,
        input: (PendingPage, &MaterialImage),
    ) -> Result<(), FrameError> {
        let (pending, image) = input;
        let (buffer, allocation) = &self.buffers[self.next];
        self.next += 1;
        let source = self
            .registry
            .buffer_view(*allocation, 0..buffer.size(), 1)?;
        let bytes = image
            .texels()
            .iter()
            .flat_map(|texel| texel.color.into_iter().chain(texel.normal_roughness))
            .collect();
        let upload = TypedOperation::new(
            RasterId::new(StageId::new("terrain page upload", 0), 1),
            TypedWork::Upload {
                destination: source.clone(),
                bytes,
            },
        );
        writer
            .encode_prepared(
                device,
                encoder,
                cache,
                PageInput {
                    registry: &mut self.registry,
                    pending,
                    source,
                    producers: vec![upload],
                },
            )
            .map_err(super::super::preparation_error)
    }
}
fn key(allocation: AllocationKey) -> WriteKey {
    WriteKey::Resource {
        id: allocation.id(),
        generation: allocation.generation(),
    }
}
