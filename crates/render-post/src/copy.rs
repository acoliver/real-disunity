mod checked;
use std::sync::{Arc, OnceLock};

use crate::{
    FrameTargets, FullscreenDescriptor, PostError, Resources, Stage, StageFactory, StageSet,
    StageTarget, fullscreen_pipeline,
};

/// No exposure, tone curve, bloom or filtering. The attachment format performs transfer encoding.
/// Nearest texel scaling also supports a differently sized native output surface.
/// Invalid output formats are reported by `StageFactory::build`, before any pipeline allocation.
#[derive(Clone, Debug)]
pub struct CopyOutput {
    format: wgpu::TextureFormat,
    bottom_first: bool,
    resources: Result<Arc<OnceLock<CopyResources>>, PostError>,
}

impl CopyOutput {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            bottom_first: false,
            resources: validate_output(device, format).map(|()| Arc::new(OnceLock::new())),
        }
    }
    /// Nearest copy from a texture whose first stored row is the displayed bottom row.
    pub fn bottom_first(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            bottom_first: true,
            ..Self::new(device, format)
        }
    }

    /// Validate source and destination before allocating targets or pipeline storage.
    pub fn validate_config(
        &self,
        device: &wgpu::Device,
        config: crate::FrameConfig,
    ) -> Result<(), PostError> {
        self.resources.as_ref().map_err(Clone::clone)?;
        if !is_float(config.format, device.features()) {
            return Err(PostError::CopySourceFormat(config.format));
        }
        Ok(())
    }
}

/// A bound nearest-copy draw for an executor-owned single-sample colour pass.
/// The caller declares the sampled resource and the destination format in its feature contract.
pub struct CopySource {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
}

impl CopySource {
    pub fn record(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl CopyOutput {
    /// Binds a checked attachment subresource for a nearest-copy feature draw.
    /// Sampling an sRGB source decodes it to linear; the destination performs its transfer.
    ///
    /// # Errors
    /// Refuses multisampled or unsampleable sources and unsupported colour formats.
    pub fn source(
        &self,
        device: &wgpu::Device,
        source: &crate::AttachmentView,
    ) -> Result<CopySource, PostError> {
        if source.texture().sample_count() != 1
            || !source
                .texture()
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(PostError::CopySourceFormat(source.format()));
        }
        self.validate_config(
            device,
            crate::FrameConfig::new(source.size(), source.format(), 1),
        )?;
        self.bind_source(device, &source.sampled_view())
    }

    fn bind_source(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
    ) -> Result<CopySource, PostError> {
        let resources = self.resources.as_ref().map_err(Clone::clone)?;
        let resources =
            resources.get_or_init(|| CopyResources::new(device, self.format, self.bottom_first));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post copy source"),
            layout: &resources.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            }],
        });
        Ok(CopySource {
            pipeline: resources.pipeline.clone(),
            group,
        })
    }
}

#[derive(Clone, Debug)]
struct CopyResources {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl CopyResources {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, bottom_first: bool) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post copy source"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post copy output"),
            source: wgpu::ShaderSource::Wgsl(include_str!("copy.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post copy output"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = fullscreen_pipeline(
            device,
            FullscreenDescriptor {
                label: Some("post copy output"),
                layout: &pipeline_layout,
                module: &module,
                vertex_entry: "vertex",
                fragment_entry: "fragment",
                format,
                constants: if bottom_first {
                    &[("bottom_first", 1.0)]
                } else {
                    &[]
                },
            },
        );
        Self { layout, pipeline }
    }
}

fn validate_output(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<(), PostError> {
    if !is_float(format, device.features())
        || !device.features().contains(format.required_features())
        || !format
            .guaranteed_format_features(device.features())
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    {
        return Err(PostError::CopyOutputFormat(format));
    }
    Ok(())
}

fn is_float(format: wgpu::TextureFormat, features: wgpu::Features) -> bool {
    // textureLoad and the non-filtering layout accept both filterable and unfilterable floats.
    matches!(
        format.sample_type(None, Some(features)),
        Some(wgpu::TextureSampleType::Float { .. })
    )
}

impl StageFactory for CopyOutput {
    fn validate_config(
        &self,
        device: &wgpu::Device,
        config: crate::FrameConfig,
    ) -> Result<(), PostError> {
        Self::validate_config(self, device, config)
    }
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        self.validate_config(device, targets.config())?;
        let source = self.bind_source(device, targets.resolved())?;
        Ok(StageSet {
            stages: vec![Stage::fullscreen(
                crate::StageId::new("post copy output", 0),
                &source.pipeline,
                &[&source.group],
                StageTarget::Output,
            )],
            resources: Resources::default(),
        })
    }
}
