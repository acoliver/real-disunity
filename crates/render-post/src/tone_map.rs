use crate::{
    FrameTargets, FullscreenDescriptor, PostError, Resources, Stage, StageFactory, StageSet,
    StageTarget, fullscreen_pipeline,
};

/// Explicit display curve. Authored filmic curves are not installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToneMap {
    Reinhard,
    Filmic,
}

/// Fixed linear exposure followed by component-wise Reinhard: x = max(radiance, 0) * exposure;
/// output = x / (1 + x). The sRGB attachment alone performs transfer encoding.
/// Negative radiance (including negative infinity) and NaN map to zero. Positive
/// infinity maps to the curve's limit, one. Zero exposure maps all radiance to zero.
#[derive(Clone, Debug)]
pub struct ToneMapOutput {
    format: wgpu::TextureFormat,
    exposure: f64,
    post: Option<Box<crate::hdr_post::HdrPost>>,
}
impl ToneMapOutput {
    pub fn new(
        format: wgpu::TextureFormat,
        exposure: f64,
        tone_map: ToneMap,
    ) -> Result<Self, PostError> {
        if tone_map != ToneMap::Reinhard {
            return Err(PostError::UnsupportedProfile("tone map"));
        }
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err(PostError::CopyOutputFormat(format));
        }
        if !exposure.is_finite() || !(0.0..=4.0).contains(&exposure) {
            return Err(PostError::UnsupportedProfile("fixed exposure"));
        }
        Ok(Self {
            format,
            exposure,
            post: None,
        })
    }

    pub fn with_effects(
        mut self,
        device: &wgpu::Device,
        settings: crate::HdrPostSettings,
    ) -> Result<Self, PostError> {
        settings.validate()?;
        self.post = if settings.enabled() {
            Some(Box::new(crate::hdr_post::HdrPost::new(device, settings)?))
        } else {
            None
        };
        Ok(self)
    }
    /// Validate the effective delta (finite, 0..=10 seconds) before any upload.
    /// A fixed capture delta overrides host timing, including invalid host values.
    pub fn prepare_effects(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
        delta: f32,
        reset: bool,
    ) -> Result<(), PostError> {
        if let Some(post) = &self.post {
            post.prepare(encoder, slot, delta, reset, 1.0)?;
        } else {
            crate::hdr_post::validate_delta(delta)?;
        }
        Ok(())
    }
    /// Published exposure result for an admitted slot, suitable for asynchronous capture readback.
    pub fn exposure_buffer(&self, slot: usize) -> Option<&wgpu::Buffer> {
        self.post.as_ref().map(|post| &post.exposure[slot])
    }
    /// Check a scene's f32 uniform against the compiled fixed multiplier.
    pub fn matches_exposure(&self, exposure: f32) -> bool {
        (f64::from(exposure) - self.exposure).abs()
            <= f64::from(f32::EPSILON) * self.exposure.max(1.0)
    }

    /// CPU reference in display-linear space, before the attachment's sRGB transfer.
    /// Exposure must satisfy the constructor's finite, nonnegative range. NaN and
    /// negative radiance map to zero; positive infinity maps to one unless exposure
    /// is zero, which maps every input to zero. Positive exposure overflow maps to one.
    pub fn evaluate(radiance: f64, exposure: f64) -> f64 {
        if exposure == 0.0 || radiance.is_nan() || radiance <= 0.0 {
            return 0.0;
        }
        let value = radiance * exposure;
        if value.is_infinite() {
            return 1.0;
        }
        value / (1.0 + value)
    }
}
impl StageFactory for ToneMapOutput {
    fn frame_slots(&self) -> usize {
        if self.post.is_some() { 3 } else { 1 }
    }
    fn build_for_slot(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
        slot: usize,
    ) -> Result<StageSet, PostError> {
        if targets.config().format != wgpu::TextureFormat::Rgba16Float {
            return Err(PostError::CopySourceFormat(targets.config().format));
        }
        if let Some(post) = &self.post {
            Ok(post.build(device, targets, slot, self.format, self.exposure))
        } else {
            self.build(device, targets, persistent)
        }
    }
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        if targets.config().format != wgpu::TextureFormat::Rgba16Float {
            return Err(PostError::CopySourceFormat(targets.config().format));
        }
        if let Some(post) = &self.post {
            return Ok(post.build(device, targets, 0, self.format, self.exposure));
        }
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fixed exposure and Reinhard output"),
            source: wgpu::ShaderSource::Wgsl(include_str!("tone_map.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HDR output source"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HDR output"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = fullscreen_pipeline(
            device,
            FullscreenDescriptor {
                label: Some("HDR output"),
                layout: &pipeline_layout,
                module: &module,
                vertex_entry: "vertex",
                fragment_entry: "fragment",
                format: self.format,
                constants: &[("EXPOSURE", self.exposure)],
            },
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HDR output"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(targets.resolved()),
            }],
        });
        Ok(StageSet {
            stages: vec![Stage::fullscreen(
                "fixed exposure and tone mapping",
                &pipeline,
                &[&group],
                StageTarget::Output,
            )],
            resources: Resources::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_exposure_curve_and_invalid_profiles() {
        for (radiance, exposure, expected) in [
            (0.0, 2.0, 0.0),
            (1.0, 1.0, 0.5),
            (4.0, 0.5, 2.0 / 3.0),
            (-1.0, 1.0, 0.0),
            (10.0, 0.0, 0.0),
            (f64::INFINITY, 1.0, 1.0),
            (f64::NEG_INFINITY, 1.0, 0.0),
            (f64::NAN, 1.0, 0.0),
            (f64::INFINITY, 0.0, 0.0),
            (f64::NEG_INFINITY, 0.0, 0.0),
            (f64::NAN, 0.0, 0.0),
            (f64::MAX, 4.0, 1.0),
        ] {
            assert!((ToneMapOutput::evaluate(radiance, exposure) - expected).abs() < 1e-12);
        }
        for exposure in [-1.0, f64::NAN, f64::INFINITY, 4.1] {
            assert!(
                ToneMapOutput::new(
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    exposure,
                    ToneMap::Reinhard
                )
                .is_err()
            );
        }
        assert!(
            ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Filmic).is_err()
        );
    }
}
