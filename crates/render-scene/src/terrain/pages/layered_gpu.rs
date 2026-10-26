//! Typed texture-backed composition through the existing checked executor.
mod admission;
#[cfg(test)]
mod admission_tests;
#[cfg(test)]
mod mip_tests;
mod scratch;
mod source;
pub use admission::LayeredGpuError;
pub(crate) use admission::supported;
pub(crate) use scratch::LayeredScratch;
#[cfg(test)]
mod tests;
use super::{
    layered::LayeredError,
    mip_gpu::{PageEncodeError, PageInput, PageWriter},
    residency::{PendingPage, ResidentPages},
    selection::PageKey,
};
use worldrunner_render_plan::{SceneResources, SceneTerrain, TerrainPageSettings};
use worldrunner_render_post::{
    StageId,
    contracts::{
        AccessMode, BufferBinding, BufferBindingKind, BufferUse, BufferUseKind, BufferView,
        CheckedBindings, ContractError, RasterId, ResourceLifetime, ResourceRegistry,
        TypedOperation, TypedWork,
    },
};

pub const LAYERED_WGSL: &str = concat!(
    include_str!("layered_gpu.wgsl"),
    include_str!("layered_bake.wgsl")
);

/// Immutable texture texels and authored parameters, never precomposed samples.
/// Images retain independent extents and their declared transfer function.
#[derive(Debug)]
pub struct LayeredGpuSource {
    parameters: Vec<u8>,
    texels: Vec<u8>,
    images: Vec<u8>,
    bounds: [f64; 4],
}

impl LayeredGpuSource {
    pub fn new(terrain: &SceneTerrain, resources: &SceneResources) -> Result<Self, LayeredError> {
        super::layered::LayeredReference::new(terrain, resources)?;
        source::encode(terrain, resources)
    }

    pub(crate) fn buffers(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<wgpu::Buffer> {
        [&self.parameters, &self.texels, &self.images]
            .into_iter()
            .map(|bytes| {
                crate::upload::upload_buffer(
                    device,
                    queue,
                    "layered terrain source",
                    wgpu::BufferUsages::STORAGE,
                    bytes,
                )
            })
            .collect()
    }

    pub fn input_bytes(&self) -> u64 {
        (self.parameters.len() + self.texels.len() + self.images.len()) as u64
    }

    /// CPU metadata consists only of sample positions; all filtering and material
    /// composition run in WGSL. The writer publishes all three products together.
    pub fn encode_page(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        cache: &ResidentPages,
        pending: PendingPage,
    ) -> Result<(), PageEncodeError> {
        let [w, h] = cache.settings().extent();
        self.validate_positions(device, u64::from(w) * u64::from(h))?;
        if cache.product_count() != 3 || self.input_bytes() > cache.composition_bytes() {
            return Err(PageEncodeError::Input);
        }
        let positions = self.page_positions(cache.settings(), pending.key());
        let mut registry = ResourceRegistry::default();
        let (output, producers) = self.operations(device, &mut registry, &positions)?;
        PageWriter::new_layered(device, cache.settings()).encode_prepared(
            device,
            encoder,
            cache,
            PageInput {
                registry: &mut registry,
                pending,
                source: output,
                producers,
            },
        )
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "page centers match the CPU reference f32 conversion"
    )]
    fn page_positions(&self, settings: TerrainPageSettings, key: PageKey) -> Vec<[f32; 2]> {
        let [w, h] = settings.extent();
        (0..h)
            .flat_map(|y| {
                (0..w).map(move |x| {
                    let uv = super::composition::source_uv(settings, key, [x, y]);
                    [
                        (self.bounds[0] + uv[0] * self.bounds[2]) as f32,
                        (self.bounds[1] + uv[1] * self.bounds[3]) as f32,
                    ]
                })
            })
            .collect()
    }

    pub fn operations(
        &self,
        device: &wgpu::Device,
        registry: &mut ResourceRegistry,
        positions: &[[f32; 2]],
    ) -> Result<(BufferView, Vec<TypedOperation>), LayeredGpuError> {
        self.validate_positions(device, positions.len() as u64)?;
        assert!(!positions.is_empty() && positions.iter().flatten().all(|v| v.is_finite()));
        let mut views = Vec::new();
        for size in self.buffer_sizes(positions.len() as u64) {
            views.push(allocate(device, registry, size)?);
        }
        Ok(self.operations_in(device, registry, positions, &views)?)
    }

    fn buffer_sizes(&self, count: u64) -> [u64; 5] {
        [
            self.parameters.len() as u64,
            self.texels.len() as u64,
            self.images.len() as u64,
            count * 8,
            count * 16,
        ]
    }

    fn operations_in(
        &self,
        device: &wgpu::Device,
        registry: &ResourceRegistry,
        positions: &[[f32; 2]],
        views: &[BufferView],
    ) -> Result<(BufferView, Vec<TypedOperation>), ContractError> {
        let position_bytes: Vec<_> = positions
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut operations = self.uploads(&position_bytes, views);
        let group = bindings(device, registry, views)?;
        let pipeline = pipeline(device, &group);
        let mut operation = TypedOperation::new(
            id("layered material composition"),
            TypedWork::Compute {
                pipeline,
                bindings: vec![group],
                workgroups: [
                    u32::try_from(positions.len())
                        .expect("position count")
                        .div_ceil(64),
                    1,
                    1,
                ],
            },
        );
        operation.buffers = views
            .iter()
            .enumerate()
            .map(|(i, view)| {
                if i == 4 {
                    BufferUse::write(view.clone(), BufferUseKind::Storage)
                } else {
                    BufferUse::read(view.clone(), BufferUseKind::Storage)
                }
            })
            .collect();
        operations.push(operation);
        Ok((views[4].clone(), operations))
    }
    fn uploads(&self, position_bytes: &[u8], views: &[BufferView]) -> Vec<TypedOperation> {
        [
            self.parameters.as_slice(),
            self.texels.as_slice(),
            self.images.as_slice(),
            position_bytes,
        ]
        .into_iter()
        .zip(views)
        .enumerate()
        .map(|(index, (bytes, view))| {
            TypedOperation::new(
                RasterId::new(
                    StageId::new(
                        "layered source upload",
                        u32::try_from(index).expect("four inputs"),
                    ),
                    1,
                ),
                TypedWork::Upload {
                    destination: view.clone(),
                    bytes: bytes.to_vec(),
                },
            )
        })
        .collect()
    }
}
fn allocate(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    size: u64,
) -> Result<BufferView, ContractError> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("layered terrain input"),
        size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let key = registry.import_buffer(&buffer, ResourceLifetime::FrameLocal, None)?;
    registry.buffer_view(key, 0..size, 1)
}
fn id(name: &'static str) -> RasterId {
    RasterId::new(StageId::new(name, 0), 1)
}
fn pipeline(device: &wgpu::Device, group: &CheckedBindings) -> wgpu::ComputePipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(group.layout())],
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("layered terrain composition"),
        source: wgpu::ShaderSource::Wgsl(LAYERED_WGSL.into()),
    });
    worldrunner_render_post::compute_pipeline(
        device,
        worldrunner_render_post::ComputeDescriptor {
            label: Some("layered terrain composition"),
            layout: &layout,
            module: &module,
            entry: "bake_layered",
            constants: &[],
        },
    )
}

fn bindings(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    views: &[BufferView],
) -> Result<CheckedBindings, ContractError> {
    let buffers: Vec<_> = views
        .iter()
        .enumerate()
        .map(|(i, view)| BufferBinding {
            binding: u32::try_from(i).expect("five bindings"),
            visibility: wgpu::ShaderStages::COMPUTE,
            view: view.clone(),
            kind: BufferBindingKind::Storage(if i == 4 {
                AccessMode::Write
            } else {
                AccessMode::Read
            }),
            dynamic_offset: None,
        })
        .collect();
    CheckedBindings::build_with_buffers(device, registry, &[], &buffers)
}
