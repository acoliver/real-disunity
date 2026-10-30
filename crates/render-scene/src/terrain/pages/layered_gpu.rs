//! Typed texture-backed composition through the existing checked executor.
mod admission;
#[cfg(test)]
mod admission_tests;
#[cfg(test)]
mod anisotropy_diagnostics;
#[cfg(test)]
mod anisotropy_reference;
#[cfg(test)]
mod anisotropy_tests;
#[cfg(test)]
mod channel_tests;
#[cfg(test)]
mod finite_footprint_tests;
#[cfg(test)]
mod mip_tests;
#[cfg(test)]
mod principal_tests;
#[cfg(test)]
mod resident_tests;
mod scratch;
mod source;
pub(crate) mod textures;
pub(crate) use textures::ResidentSource;
#[cfg(test)]
mod source_mip_tests;
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
    grid: Vec<u8>,
    arrays: Vec<Vec<crate::texture::OwnedPreparedTexture>>,
    resident:
        std::sync::Mutex<std::collections::HashMap<wgpu::Device, std::sync::Arc<ResidentSource>>>,
    images: Vec<u8>,
    bounds: [f64; 4],
}

impl LayeredGpuSource {
    pub fn new(terrain: &SceneTerrain, resources: &SceneResources) -> Result<Self, LayeredError> {
        terrain.validate().map_err(|_| LayeredError::Source)?;
        terrain
            .layered_source()
            .ok_or(LayeredError::Source)?
            .validate_resources(terrain, resources.textures())
            .map_err(|_| LayeredError::Texture)?;
        source::encode(terrain, resources)
    }

    pub fn input_bytes(&self) -> u64 {
        (self.parameters.len() + self.grid.len() + self.images.len()) as u64
            + textures::payload_bytes(&self.arrays)
    }

    /// CPU metadata consists only of sample positions and their footprint; all filtering and material
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
        let (output, producers) = self.operations_for(
            device,
            cache.queue(),
            &mut registry,
            (&positions.0, positions.1),
        )?;
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
    fn page_positions(
        &self,
        settings: TerrainPageSettings,
        key: PageKey,
    ) -> (Vec<[f32; 2]>, [f32; 2]) {
        let [w, h] = settings.extent();
        let positions = (0..h)
            .flat_map(|y| {
                (0..w).map(move |x| {
                    let uv = super::composition::source_uv(settings, key, [x, y]);
                    [
                        (self.bounds[0] + uv[0] * self.bounds[2]) as f32,
                        (self.bounds[1] + uv[1] * self.bounds[3]) as f32,
                    ]
                })
            })
            .collect();
        let footprint = std::array::from_fn(|axis| {
            (self.bounds[axis + 2]
                / f64::from(settings.parts().interior[axis])
                / f64::from(1_u32 << key.depth())) as f32
        });
        (positions, footprint)
    }

    pub fn operations(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        registry: &mut ResourceRegistry,
        positions: &[[f32; 2]],
    ) -> Result<(BufferView, Vec<TypedOperation>), LayeredGpuError> {
        self.operations_for(device, queue, registry, (positions, [0.0; 2]))
    }

    fn operations_for(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        registry: &mut ResourceRegistry,
        samples: (&[[f32; 2]], [f32; 2]),
    ) -> Result<(BufferView, Vec<TypedOperation>), LayeredGpuError> {
        let positions = samples.0;
        self.validate_positions(device, positions.len() as u64)?;
        assert!(!positions.is_empty() && positions.iter().flatten().all(|v| v.is_finite()));
        let mut views = self.import_source(device, queue, registry)?;
        for size in Self::buffer_sizes(positions.len() as u64) {
            views.push(allocate(device, registry, size)?);
        }
        Ok(self.operations_in(device, registry, samples, &views)?)
    }

    fn buffer_sizes(count: u64) -> [u64; 2] {
        [count * 8 + 8, count * 16]
    }

    fn operations_in(
        &self,
        device: &wgpu::Device,
        registry: &mut ResourceRegistry,
        samples: (&[[f32; 2]], [f32; 2]),
        views: &[BufferView],
    ) -> Result<(BufferView, Vec<TypedOperation>), ContractError> {
        let (positions, footprint) = samples;
        let position_bytes: Vec<_> = footprint
            .iter()
            .chain(positions.iter().flatten())
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut operations = Self::uploads(&position_bytes, views);
        let (textures, filtering) = self.texture_bindings(device, registry)?;
        let group = bindings(device, registry, views, &textures, &filtering)?;
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
        operation.textures = textures
            .into_iter()
            .map(|entry| {
                worldrunner_render_post::contracts::ResourceUse::read(
                    entry.view,
                    worldrunner_render_post::contracts::UseKind::Sampled,
                )
            })
            .collect();
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
    fn uploads(position_bytes: &[u8], views: &[BufferView]) -> Vec<TypedOperation> {
        vec![TypedOperation::new(
            id("layered positions upload"),
            TypedWork::Upload {
                destination: views[3].clone(),
                bytes: position_bytes.to_vec(),
            },
        )]
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
    textures: &[worldrunner_render_post::contracts::TextureBinding],
    samplers: &[worldrunner_render_post::contracts::SamplerBinding],
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
    CheckedBindings::build_with_resources(device, registry, textures, &buffers, samplers)
}
