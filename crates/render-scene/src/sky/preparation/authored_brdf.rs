use super::{PreparationError, PreparedEnvironmentMaps};
use std::num::NonZeroU64;
use worldrunner_render_plan::SceneSpecularConvention;

/// Texture coordinates are x = view cosine, y = material roughness. RG stores A,B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrdfTableAxes {
    ViewCosineRoughness,
}

/// Caller-owned square table identity and its linear, filterable GPU resource.
#[derive(Clone, Debug)]
pub struct AuthoredBrdfTable {
    identity: NonZeroU64,
    pub(super) view: wgpu::TextureView,
    convention: SceneSpecularConvention,
    axes: BrdfTableAxes,
}
impl AuthoredBrdfTable {
    pub fn new(
        identity: NonZeroU64,
        texture: &wgpu::Texture,
        convention: SceneSpecularConvention,
        axes: BrdfTableAxes,
    ) -> Result<Self, PreparationError> {
        if texture.width() != texture.height()
            || texture.dimension() != wgpu::TextureDimension::D2
            || texture.depth_or_array_layers() != 1
            || texture.sample_count() != 1
            || texture.mip_level_count() != 1
            || !matches!(
                texture.format(),
                wgpu::TextureFormat::Rg16Float | wgpu::TextureFormat::Rgba16Float
            )
            || !texture
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(PreparationError::Source);
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });
        Ok(Self {
            identity,
            view,
            convention,
            axes,
        })
    }
    pub const fn identity(&self) -> NonZeroU64 {
        self.identity
    }
    pub const fn axes(&self) -> BrdfTableAxes {
        self.axes
    }
}
impl PreparedEnvironmentMaps {
    /// Replace the generated table in the existing binding, without another sampled texture.
    fn with_authored_brdf(mut self, table: AuthoredBrdfTable) -> Result<Self, PreparationError> {
        if table.convention.roughness != self.convention.roughness
            || table.convention.visibility != self.convention.visibility
        {
            return Err(PreparationError::Settings);
        }
        self.authored_brdf = Some(table);
        Ok(self)
    }
    pub fn authored_brdf(&self) -> Option<&AuthoredBrdfTable> {
        self.authored_brdf.as_ref()
    }
}

/// Authored coefficients receive the same intensity once as generated coefficients.
/// The caller owns the source table allocation. Non-neutral intensity reuses the budgeted LUT.
pub fn encode_with_authored_brdf(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::TextureView,
    settings: super::EnvironmentPreparation,
    table: AuthoredBrdfTable,
    budget: worldrunner_render_gpu::configuration::AllocationBudget,
) -> Result<PreparedEnvironmentMaps, PreparationError> {
    let settings = super::EnvironmentPreparation {
        lut_size: table.view.texture().width(),
        ..settings
    };
    let maps = super::encode(device, encoder, source, settings, budget)?;
    install(device, encoder, maps, table)
}

pub(super) fn install(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    maps: PreparedEnvironmentMaps,
    table: AuthoredBrdfTable,
) -> Result<PreparedEnvironmentMaps, PreparationError> {
    let maps = maps.with_authored_brdf(table)?;
    if maps.baked_specular_intensity_micro != 1_000_000 {
        scale_table(device, encoder, &maps);
    }
    Ok(maps)
}

fn scale_table(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    maps: &PreparedEnvironmentMaps,
) {
    let pipeline = scale_pipeline(device, maps.baked_specular_intensity_micro);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let output = maps
        .lut
        .create_view(&wgpu::TextureViewDescriptor::default());
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("authored BRDF intensity"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &maps.authored_brdf.as_ref().expect("authored table").view,
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&output),
            },
        ],
    });
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &group, &[]);
    pass.dispatch_workgroups(
        maps.lut.width().div_ceil(8),
        maps.lut.height().div_ceil(8),
        1,
    );
}

fn scale_pipeline(device: &wgpu::Device, intensity: u32) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("authored BRDF intensity"),
        source: wgpu::ShaderSource::Wgsl(r"
override INTENSITY:f32=1.0;
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var linear_sampler:sampler;
@group(0) @binding(2) var output:texture_storage_2d<rgba16float,write>;
@compute @workgroup_size(8,8) fn scale(@builtin(global_invocation_id) id:vec3<u32>) {
 let size=textureDimensions(output);
 if any(id.xy>=size) { return; }
 let uv=(vec2<f32>(id.xy)+0.5)/vec2<f32>(size);
 textureStore(output,id.xy,vec4(textureSampleLevel(source,linear_sampler,uv,0.0).rg*INTENSITY,0.0,1.0));
}".into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("authored BRDF intensity"),
        layout: None,
        module: &module,
        entry_point: Some("scale"),
        compilation_options: wgpu::PipelineCompilationOptions {
            constants: &[("INTENSITY", f64::from(intensity) / 1e6)],
            ..Default::default()
        },
        cache: None,
    })
}
