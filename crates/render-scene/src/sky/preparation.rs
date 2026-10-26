//! Bounded environment filtering jobs recorded through the queue's submission authority.
//! No resource becomes observable until its transaction submits; cancellation drops the work.
use wgpu::util::DeviceExt;
use worldrunner_render_gpu::{
    configuration::{AllocationBudget, AllocationSet},
    submission::{Publications, TransactionStatus},
};

const SOURCE: &str = concat!(
    worldrunner_render_lighting::lighting_wgsl!(),
    include_str!("preparation.wgsl")
);
#[derive(Clone, Copy, Debug)]
pub struct EnvironmentPreparation {
    pub convention: worldrunner_render_plan::SceneSpecularConvention,
    pub cube_size: u32,
    pub mip_count: u32,
    pub lut_size: u32,
    pub samples: u32,
}
impl EnvironmentPreparation {
    pub fn allocations(self) -> Result<AllocationSet, PreparationError> {
        if self.cube_size == 0
            || !self.cube_size.is_power_of_two()
            || self.cube_size > 1024
            || self.mip_count == 0
            || self.mip_count > self.cube_size.ilog2() + 1
            || self.lut_size == 0
            || self.lut_size > 1024
            || self.samples == 0
            || self.samples > 4096
        {
            return Err(PreparationError::Settings);
        }
        let mut set = AllocationSet::default();
        set.texture(
            "filtered environment",
            [self.cube_size, self.cube_size, 6],
            wgpu::TextureFormat::Rgba16Float,
            [self.mip_count, 1],
            1,
        );
        set.texture(
            "integrated BRDF",
            [self.lut_size, self.lut_size, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            1,
        );
        set.buffer("environment job uniforms", 32, self.mip_count + 1);
        Ok(set)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum PreparationError {
    #[error("invalid environment preparation settings")]
    Settings,
    #[error("environment allocation exceeds budget or device limits")]
    Budget,
    #[error("environment source must be a linear floating-point cube")]
    Source,
    #[error(transparent)]
    Transaction(#[from] worldrunner_render_gpu::submission::TransactionError),
    #[error("environment preparation has not submitted")]
    NotSubmitted,
}
#[derive(Debug)]
pub struct PreparedEnvironmentMaps {
    cube: wgpu::Texture,
    lut: wgpu::Texture,
    transaction: Publications,
    convention: worldrunner_render_plan::SceneSpecularConvention,
}
impl PreparedEnvironmentMaps {
    pub const fn convention(&self) -> worldrunner_render_plan::SceneSpecularConvention {
        self.convention
    }
    /// Queue ordering permits use after submission without a CPU wait.
    pub fn views(&self) -> Result<(wgpu::TextureView, wgpu::TextureView), PreparationError> {
        if self.transaction.status() != TransactionStatus::Submitted {
            return Err(PreparationError::NotSubmitted);
        }
        Ok((
            self.cube.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            }),
            self.lut
                .create_view(&wgpu::TextureViewDescriptor::default()),
        ))
    }
    pub fn status(&self) -> TransactionStatus {
        self.transaction.status()
    }
}

/// Record both jobs in the current authority-owned encoder. Source storage remains caller-owned
/// and must already be included in that caller's budget; this budget covers generated resources.
pub fn encode(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::TextureView,
    settings: EnvironmentPreparation,
    budget: AllocationBudget,
) -> Result<PreparedEnvironmentMaps, PreparationError> {
    let transaction = Publications::for_encoder(encoder)?;
    let allocations = settings.allocations()?;
    let required = allocations
        .validate(&device.limits())
        .map_err(|_| PreparationError::Budget)?;
    if required > budget.0 {
        return Err(PreparationError::Budget);
    }
    if source.texture().width() != source.texture().height()
        || source.texture().depth_or_array_layers() != 6
        || !matches!(source.texture().format(), wgpu::TextureFormat::Rgba16Float)
        || !source
            .texture()
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
    {
        return Err(PreparationError::Source);
    }
    let cube = texture(device, settings.cube_size, 6, settings.mip_count);
    let lut = texture(device, settings.lut_size, 1, 1);
    let job = Integration::new(device, source);
    for mip in 0..=settings.mip_count {
        job.dispatch(
            encoder,
            if mip == settings.mip_count {
                &lut
            } else {
                &cube
            },
            settings,
            mip,
        );
    }
    Ok(PreparedEnvironmentMaps {
        cube,
        lut,
        transaction,
        convention: settings.convention,
    })
}
struct Integration<'a> {
    device: &'a wgpu::Device,
    source: &'a wgpu::TextureView,
    pipeline: wgpu::ComputePipeline,
    sampler: wgpu::Sampler,
}
impl<'a> Integration<'a> {
    fn new(device: &'a wgpu::Device, source: &'a wgpu::TextureView) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("environment integration"),
            source: wgpu::ShaderSource::Wgsl(SOURCE.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("environment integration"),
            layout: None,
            module: &shader,
            entry_point: Some("integrate"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("environment source"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self {
            device,
            source,
            pipeline,
            sampler,
        }
    }
    fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::Texture,
        settings: EnvironmentPreparation,
        mip: u32,
    ) {
        let is_lut = mip == settings.mip_count;
        let size = if is_lut {
            settings.lut_size
        } else {
            settings.cube_size >> mip
        };
        let layers = if is_lut { 1 } else { 6 };
        let view = output.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            base_mip_level: if is_lut { 0 } else { mip },
            mip_level_count: Some(1),
            ..Default::default()
        });
        let roughness = roughness(mip, settings.mip_count);
        let bytes = [
            size,
            settings.samples,
            u32::from(is_lut),
            layers,
            roughness.to_bits(),
            settings.convention.shader_options()[0],
            settings.convention.shader_options()[1],
            0,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
        let group = self.bind(&view, &bytes);
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("environment integration"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(size.div_ceil(8), size.div_ceil(8), layers);
    }
    fn bind(&self, view: &wgpu::TextureView, bytes: &[u8]) -> wgpu::BindGroup {
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("environment integration parameters"),
                contents: bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("environment integration"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(self.source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        })
    }
}
#[expect(clippy::cast_precision_loss, reason = "at most eleven mip levels")]
fn roughness(mip: u32, count: u32) -> f32 {
    if count == 1 {
        0.0
    } else {
        mip as f32 / (count - 1) as f32
    }
}
fn texture(device: &wgpu::Device, size: u32, layers: u32, mips: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("prepared environment"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: layers,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Single-scattering columns prepared under the same authority as environment maps.
#[derive(Debug)]
pub struct PreparedAtmosphere {
    lookup: super::scattering::Lookup,
    transaction: Publications,
}
impl PreparedAtmosphere {
    /// A 1-by-rows array: molecular integral, aerosol integral, view transmission.
    /// The first two layers are phase-independent column lengths in metres.
    pub fn view(&self) -> Result<wgpu::TextureView, PreparationError> {
        if self.status() != TransactionStatus::Submitted {
            return Err(PreparationError::NotSubmitted);
        }
        Ok(self.lookup.view.clone())
    }
    pub fn status(&self) -> TransactionStatus {
        self.transaction.status()
    }
}

pub fn atmosphere_allocations(
    settings: worldrunner_render_plan::SceneScattering,
) -> Result<AllocationSet, PreparationError> {
    settings
        .validate()
        .map_err(|_| PreparationError::Settings)?;
    Ok(super::scattering::allocations(settings))
}

/// The observer pair contains a unit toward-sun direction and altitude in metres.
/// Cancellation discards the upload and prevents publication of the texture view.
pub fn encode_atmosphere(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    settings: worldrunner_render_plan::SceneScattering,
    observer: (glam::Vec3, f32),
    budget: AllocationBudget,
) -> Result<PreparedAtmosphere, PreparationError> {
    let transaction = Publications::for_encoder(encoder)?;
    let required = atmosphere_allocations(settings)?
        .validate(&device.limits())
        .map_err(|_| PreparationError::Budget)?;
    if required > budget.0 {
        return Err(PreparationError::Budget);
    }
    let lookup = super::scattering::Lookup::new(device, settings);
    lookup.encode(device, encoder, observer, budget)?;
    Ok(PreparedAtmosphere {
        lookup,
        transaction,
    })
}
#[cfg(test)]
mod tests;
