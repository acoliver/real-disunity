use crate::{FrameTargets, PostError, Resources, StageFactory, StageSet, StageTarget};

/// Internal/output ratio in millionths, from 0.5 through 1.0 inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderScale(u32);
impl RenderScale {
    pub fn new(millionths: u32) -> Result<Self, PostError> {
        if !(500_000..=1_000_000).contains(&millionths) {
            return Err(PostError::UnsupportedProfile(
                "render scale must be 500000..=1000000 millionths",
            ));
        }
        Ok(Self(millionths))
    }
    /// Round to nearest integer, with halfway values rounded up.
    pub fn extent(self, output: [u32; 2]) -> Result<[u32; 2], PostError> {
        if output.contains(&0) {
            return Err(PostError::EmptyExtent);
        }
        Ok(output.map(|side| {
            u32::try_from((u64::from(side) * u64::from(self.0) + 500_000) / 1_000_000)
                .expect("validated scale cannot exceed the output extent")
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpscaleMode {
    Bilinear,
    Easu,
}

/// Spatial reconstruction of display-referred RGB. `None` disables RCAS exactly.
/// RCAS sharpness is in stops: zero is maximum; the supported range is 0..=2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpscaleSettings {
    pub mode: UpscaleMode,
    pub sharpness: Option<f32>,
}
impl UpscaleSettings {
    pub fn validate(self) -> Result<(), PostError> {
        if self
            .sharpness
            .is_some_and(|v| !v.is_finite() || !(0.0..=2.0).contains(&v))
        {
            return Err(PostError::UnsupportedProfile(
                "RCAS sharpness must be finite and within 0..=2 stops",
            ));
        }
        Ok(())
    }
}

/// Standalone factory for an already tone-mapped source. HDR callers use `HdrPostSettings`.
#[derive(Clone, Debug)]
pub struct SpatialUpscale {
    pub settings: UpscaleSettings,
    pub output_format: wgpu::TextureFormat,
}
impl StageFactory for SpatialUpscale {
    fn validate_config(
        &self,
        device: &wgpu::Device,
        config: crate::FrameConfig,
    ) -> Result<(), PostError> {
        self.settings.validate()?;
        crate::CopyOutput::new(device, self.output_format).validate_config(device, config)
    }
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        self.validate_config(device, targets.config())?;
        let mut set = StageSet::default();
        append(
            device,
            &mut set,
            targets.resolved(),
            targets.config().output_extent(),
            self,
        );
        Ok(set)
    }
}

pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    source: &wgpu::TextureView,
    output: [u32; 2],
    factory: &SpatialUpscale,
) {
    let sharpness = factory.settings.sharpness;
    let module = module(device, "FSR 1 spatial reconstruction");
    // Half precision between display-referred passes halves their memory traffic.
    let intermediate = sharpness.map(|_| {
        let texture = crate::color_texture(
            device,
            "RCAS input",
            output,
            wgpu::TextureFormat::Rgba16Float,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        set.resources.textures.push(texture);
        view
    });
    let entry = match factory.settings.mode {
        UpscaleMode::Bilinear => "bilinear",
        UpscaleMode::Easu => "easu",
    };
    pass(
        device,
        &module,
        set,
        source,
        Pass {
            entry,
            output,
            format: intermediate
                .as_ref()
                .map_or(factory.output_format, |_| wgpu::TextureFormat::Rgba16Float),
            target: intermediate
                .as_ref()
                .map_or(StageTarget::Output, |v| StageTarget::Texture(v.clone())),
            sharpness: 0.0,
            encoded_srgb: false,
        },
    );
    if let (Some(stops), Some(view)) = (sharpness, intermediate.as_ref()) {
        pass(
            device,
            &module,
            set,
            view,
            Pass {
                entry: "rcas",
                output,
                format: factory.output_format,
                target: StageTarget::Output,
                sharpness: stops,
                encoded_srgb: false,
            },
        );
    }
}
pub(crate) fn append_native(
    device: &wgpu::Device,
    set: &mut StageSet,
    source: &wgpu::TextureView,
    settings: crate::NativeRcasSettings,
    format: wgpu::TextureFormat,
) {
    let module = module(device, "native RCAS");
    pass(
        device,
        &module,
        set,
        source,
        Pass {
            entry: "rcas",
            output: [source.texture().width(), source.texture().height()],
            format,
            target: StageTarget::Output,
            sharpness: settings.stops,
            encoded_srgb: settings.domain == crate::RcasDomain::EncodedOutput && format.is_srgb(),
        },
    );
}

fn module(device: &wgpu::Device, label: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(include_str!("upscale.wgsl").into()),
    })
}

struct Pass {
    entry: &'static str,
    output: [u32; 2],
    format: wgpu::TextureFormat,
    target: StageTarget,
    sharpness: f32,
    encoded_srgb: bool,
}
fn pass(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    set: &mut StageSet,
    source: &wgpu::TextureView,
    p: Pass,
) {
    let layout = source_layout(device, p.entry);
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(p.entry),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = crate::fullscreen_pipeline(
        device,
        crate::FullscreenDescriptor {
            label: Some(p.entry),
            layout: &pipeline_layout,
            module,
            vertex_entry: "vertex",
            fragment_entry: p.entry,
            format: p.format,
            constants: &[
                ("OUTPUT_WIDTH", f64::from(p.output[0])),
                ("OUTPUT_HEIGHT", f64::from(p.output[1])),
                ("SHARPNESS", p.sharpness.into()),
                ("ENCODED_SRGB", f64::from(u8::from(p.encoded_srgb))),
            ],
        },
    );
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(p.entry),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(source),
        }],
    });
    set.stages.push(crate::Stage::fullscreen(
        crate::StageId::new(p.entry, 0),
        &pipeline,
        &[&group],
        p.target,
    ));
}

pub(crate) fn source_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_rounding_and_rejections() {
        for scale in [0, 499_999, 1_000_001, u32::MAX] {
            assert!(RenderScale::new(scale).is_err());
        }
        assert_eq!(
            RenderScale::new(500_000).unwrap().extent([1, 1]),
            Ok([1, 1])
        );
        assert_eq!(
            RenderScale::new(770_000).unwrap().extent([50, 26]),
            Ok([39, 20])
        );
        assert_eq!(
            RenderScale::new(1_000_000).unwrap().extent([33, 17]),
            Ok([33, 17])
        );
        assert_eq!(
            RenderScale::new(500_000).unwrap().extent([0, 1]),
            Err(PostError::EmptyExtent)
        );
    }
    #[test]
    fn sharpness_validation() {
        for sharpness in [f32::NAN, f32::INFINITY, -0.01, 2.01] {
            assert!(
                UpscaleSettings {
                    mode: UpscaleMode::Easu,
                    sharpness: Some(sharpness)
                }
                .validate()
                .is_err()
            );
        }
        for sharpness in [None, Some(0.0), Some(2.0)] {
            assert!(
                UpscaleSettings {
                    mode: UpscaleMode::Easu,
                    sharpness
                }
                .validate()
                .is_ok()
            );
        }
    }
}
