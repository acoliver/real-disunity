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
    resources: Result<Arc<OnceLock<CopyResources>>, PostError>,
}

impl CopyOutput {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            resources: validate_output(device, format).map(|()| Arc::new(OnceLock::new())),
        }
    }
}

#[derive(Clone, Debug)]
struct CopyResources {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

impl CopyResources {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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
                constants: &[],
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
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        let resources = self.resources.as_ref().map_err(Clone::clone)?;
        let format = targets.config().format;
        if !is_float(format, device.features()) {
            return Err(PostError::CopySourceFormat(format));
        }
        // Validate both formats before allocation; clones and resizes reuse the pipeline.
        let resources = resources.get_or_init(|| CopyResources::new(device, self.format));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post copy source"),
            layout: &resources.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(targets.resolved()),
            }],
        });
        Ok(StageSet {
            stages: vec![Stage::fullscreen(
                "post copy output",
                &resources.pipeline,
                &[&group],
                StageTarget::Output,
            )],
            resources: Resources::default(),
        })
    }
}
