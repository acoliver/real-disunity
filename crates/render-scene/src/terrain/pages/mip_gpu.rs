//! Reusable executor pipelines that store packed base products and build resident mips.
mod operations;
mod pipelines;
use super::{
    composition::MaterialImage,
    residency::{PageCacheError, PendingPage, ResidentPages},
};
use worldrunner_render_post::{
    StageId,
    contracts::{
        BufferView, ContractError, RasterId, ResourceLifetime, ResourceRegistry, TypedOperation,
        TypedWork,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum PageEncodeError {
    #[error(transparent)]
    Layered(#[from] super::layered_gpu::LayeredGpuError),
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error(transparent)]
    Cache(#[from] PageCacheError),
    #[error("page input dimensions or reservation do not match the cache")]
    Input,
}

pub struct PageWriter {
    store: wgpu::ComputePipeline,
    reduce: wgpu::ComputePipeline,
    input_bytes: u64,
    products: u32,
}
impl PageWriter {
    pub fn new(
        device: &wgpu::Device,
        settings: worldrunner_render_plan::TerrainPageSettings,
    ) -> Self {
        let [w, h] = settings.extent();
        pipelines::create(device, u64::from(w) * u64::from(h) * 8, 2)
    }

    pub fn new_layered(
        device: &wgpu::Device,
        settings: worldrunner_render_plan::TerrainPageSettings,
    ) -> Self {
        let [w, h] = settings.extent();
        pipelines::create(device, u64::from(w) * u64::from(h) * 16, 3)
    }

    /// Uploads already-composed products. Layer composition can instead supply a
    /// checked GPU buffer with `encode_prepared`, avoiding this upload.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        cache: &ResidentPages,
        pending: PendingPage,
        base: &MaterialImage,
    ) -> Result<(), PageEncodeError> {
        if base.extent() != cache.settings().extent() {
            return Err(PageEncodeError::Input);
        }
        let bytes: Vec<_> = base
            .texels()
            .iter()
            .flat_map(|texel| texel.color.into_iter().chain(texel.normal_roughness))
            .collect();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain page packed input"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let mut registry = ResourceRegistry::default();
        let key = registry.import_buffer(&buffer, ResourceLifetime::FrameLocal, None)?;
        let source = registry.buffer_view(key, 0..buffer.size(), 1)?;
        let upload = TypedOperation::new(
            id("terrain page upload", 0),
            TypedWork::Upload {
                destination: source.clone(),
                bytes,
            },
        );
        self.encode_prepared(
            device,
            encoder,
            cache,
            PageInput {
                registry: &mut registry,
                pending,
                source,
                producers: vec![upload],
            },
        )
    }

    /// Input producers and resident writes compile as one checked operation list.
    /// The caller admits layer composition inputs separately; packed input and
    /// upload staging are included in the cache configuration allocation set.
    pub fn encode_prepared(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        cache: &ResidentPages,
        input: PageInput<'_>,
    ) -> Result<(), PageEncodeError> {
        let PageInput {
            registry,
            pending,
            source,
            mut producers,
        } = input;
        pending.validate(encoder, &cache.storage().state)?;
        let [w, h] = cache.settings().extent();
        if source.range().end - source.range().start != self.input_bytes
            || self.products != cache.product_count()
            || self.input_bytes
                != u64::from(w) * u64::from(h) * if self.products == 3 { 16 } else { 8 }
        {
            return Err(PageEncodeError::Input);
        }
        producers.extend(operations::build(
            self, device, registry, cache, &pending, &source,
        )?);
        let compiled = registry.compile_operations(device, &producers)?;
        compiled.encode(registry, encoder, None, |_, _| {})?;
        pending.stage(encoder)?;
        Ok(())
    }
}

pub struct PageInput<'a> {
    pub registry: &'a mut ResourceRegistry,
    pub pending: PendingPage,
    pub source: BufferView,
    pub producers: Vec<TypedOperation>,
}
fn id(name: &'static str, mip: u32) -> RasterId {
    RasterId::new(StageId::new(name, mip), 1)
}
