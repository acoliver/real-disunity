use super::{
    BufferView, LayeredGpuError, LayeredGpuSource, PageEncodeError, PageInput, PageWriter,
    PendingPage, ResidentPages, ResourceRegistry, allocate,
};
use worldrunner_render_gpu::submission::capture::WriteKey;

pub(crate) struct LayeredScratch {
    registry: ResourceRegistry,
    views: Vec<BufferView>,
    writer: PageWriter,
    texture: worldrunner_render_post::contracts::AllocationKey,
}

impl LayeredScratch {
    pub fn new(
        device: &wgpu::Device,
        source: &LayeredGpuSource,
        cache: &ResidentPages,
    ) -> Result<Self, LayeredGpuError> {
        let settings = cache.settings();
        let [w, h] = settings.extent();
        let count = u64::from(w) * u64::from(h);
        source.validate_positions(device, count)?;
        let mut registry = ResourceRegistry::default();
        let storage = cache.storage();
        let texture = registry.import_leased(
            &storage.texture,
            &storage.owner.lease(),
            worldrunner_render_post::contracts::Semantic::Unspecified,
            None,
        )?;
        let mut views = source.import_source(device, cache.queue(), &mut registry)?;
        for size in LayeredGpuSource::buffer_sizes(count) {
            views.push(allocate(device, &mut registry, size)?);
        }
        Ok(Self {
            registry,
            texture,
            views,
            writer: PageWriter::new_layered(device, settings),
        })
    }

    pub fn keys(&self) -> impl Iterator<Item = WriteKey> + '_ {
        std::iter::once(self.texture)
            .chain(self.views[3..].iter().map(BufferView::allocation))
            .map(|key| WriteKey::Resource {
                id: key.id(),
                generation: key.generation(),
            })
    }

    pub fn encode(
        &mut self,
        source: &LayeredGpuSource,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        cache: &ResidentPages,
        pending: PendingPage,
    ) -> Result<(), PageEncodeError> {
        let positions = source.page_positions(cache.settings(), pending.key());
        let (output, producers) = source.operations_in(
            device,
            &mut self.registry,
            (&positions.0, positions.1),
            &self.views,
        )?;
        self.writer.encode_prepared(
            device,
            encoder,
            cache,
            PageInput {
                registry: &mut self.registry,
                pending,
                source: output,
                producers,
            },
        )
    }
}
