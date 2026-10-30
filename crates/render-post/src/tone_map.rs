use crate::{
    FrameTargets, FullscreenDescriptor, PostError, Resources, Stage, StageFactory, StageSet,
    StageTarget, fullscreen_pipeline,
};

/// Explicit display curve, applied after exposure and before attachment encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToneMap {
    Reinhard,
    /// Clamped fitted ACES approximation by Krzysztof Narkowicz (2016).
    Filmic,
    /// Stephen Hill's matrix RRT/ODT fit, published in `BakingLab` under MIT:
    /// <https://github.com/TheRealMJP/BakingLab/blob/master/BakingLab/ACES.hlsl>.
    AcesFitted,
}

impl ToneMap {
    pub(crate) fn constant(self) -> f64 {
        match self {
            Self::Reinhard => 0.0,
            Self::Filmic => 1.0,
            Self::AcesFitted => 2.0,
        }
    }
    /// Display-linear reference. Exposure must satisfy the output constructor's range.
    pub fn evaluate(self, radiance: f64, exposure: f64) -> f64 {
        if self == Self::AcesFitted {
            return self.evaluate_rgb([radiance; 3], exposure)[0];
        }
        if self == Self::Reinhard {
            return ToneMapOutput::evaluate(radiance, exposure);
        }
        if exposure == 0.0 || radiance.is_nan() || radiance <= 0.0 {
            return 0.0;
        }
        let x = radiance * exposure;
        // The fitted curve exceeds one by x=16; saturation avoids quadratic overflow.
        if x >= 16.0 {
            return 1.0;
        }
        crate::aces::fitted(x).clamp(0.0, 1.0)
    }
    /// RGB reference. Matrix fitting mixes channels; scalar evaluate uses neutral RGB.
    pub fn evaluate_rgb(self, rgb: [f64; 3], exposure: f64) -> [f64; 3] {
        if self != Self::AcesFitted {
            return rgb.map(|v| self.evaluate(v, exposure));
        }
        if exposure == 0.0 {
            return [0.0; 3];
        }
        crate::aces::matrix_fit(rgb.map(|v| crate::display_grading::sanitize(v) * exposure))
    }
}

/// Fixed linear exposure followed by the selected display curve.
/// The sRGB attachment alone performs transfer encoding.
/// Negative radiance (including negative infinity) and NaN map to zero. Positive
/// infinity maps to the curve's limit, one. Zero exposure maps all radiance to zero.
#[derive(Clone, Debug)]
pub struct ToneMapOutput {
    format: wgpu::TextureFormat,
    exposure: f64,
    tone_map: ToneMap,
    post: Option<Box<crate::hdr_post::HdrPost>>,
}
impl ToneMapOutput {
    pub fn new(
        format: wgpu::TextureFormat,
        exposure: f64,
        tone_map: ToneMap,
    ) -> Result<Self, PostError> {
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
            tone_map,
            post: None,
        })
    }

    pub fn with_effects(
        mut self,
        device: &wgpu::Device,
        settings: crate::HdrPostSettings,
    ) -> Result<Self, PostError> {
        settings.validate()?;
        if let Some(curve) = settings.tone_map {
            self.tone_map = curve;
        }
        self.post = if settings.enabled() {
            Some(Box::new(crate::hdr_post::HdrPost::new(device, &settings)?))
        } else {
            None
        };
        Ok(self)
    }
    /// Rebind settings while retaining queue-ordered exposure and slot history.
    pub fn with_retained_effects(
        mut self,
        device: &wgpu::Device,
        settings: crate::HdrPostSettings,
        previous: Option<&Self>,
    ) -> Result<Self, PostError> {
        settings.validate()?;
        if let Some(curve) = settings.tone_map {
            self.tone_map = curve;
        }
        self.post = if let Some(post) = previous.and_then(|previous| previous.post.as_ref()) {
            let mut retained = post.clone();
            retained.set_settings(&settings);
            Some(retained)
        } else if settings.enabled() {
            Some(Box::new(crate::hdr_post::HdrPost::new(device, &settings)?))
        } else {
            None
        };
        Ok(self)
    }
    pub fn has_effect_history(&self) -> bool {
        self.post.is_some()
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
    /// Uploads the TAA block of `slot`; `None` uploads zeros, so the resolve ignores history.
    /// Without effects there is nothing to upload to.
    pub fn prepare_taa(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
        block: Option<&[u8; crate::TAA_UNIFORM_BYTES]>,
    ) {
        if let Some(post) = &self.post {
            post.prepare_taa(encoder, slot, block);
        }
    }
    /// Uploads the temporal block of `slot`; `None` uploads zeros, so the reconstruction ignores
    /// history. Without effects there is nothing to upload to.
    ///
    /// # Errors
    ///
    /// [`PostError::UnsupportedProfile`] for an invalid block.
    pub fn prepare_temporal(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
        block: Option<&crate::TemporalBlock>,
    ) -> Result<(), PostError> {
        self.post
            .as_ref()
            .map_or(Ok(()), |post| post.prepare_temporal(encoder, slot, block))
    }
    #[cfg(test)]
    pub(crate) fn taa_history_size(&self) -> Option<[u32; 2]> {
        self.post
            .as_deref()
            .and_then(crate::hdr_post::HdrPost::taa_history_size)
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
    fn validate_config(
        &self,
        _: &wgpu::Device,
        config: crate::FrameConfig,
    ) -> Result<(), PostError> {
        if config.format != wgpu::TextureFormat::Rgba16Float {
            return Err(PostError::CopySourceFormat(config.format));
        }
        self.post
            .as_ref()
            .map_or_else(crate::HdrPostSettings::default, |post| post.settings)
            .validate_frame(config)
    }

    fn frame_slots(&self) -> usize {
        if self.post.is_some() { 3 } else { 1 }
    }
    fn needs_velocity(&self) -> bool {
        self.post
            .as_ref()
            .is_some_and(|post| post.settings.resolve.active_taa().is_some())
    }
    fn settle_build(&self, accepted: bool) {
        if let Some(post) = &self.post {
            post.settle_build(accepted);
        }
    }
    fn build_for_slot(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
        slot: usize,
    ) -> Result<StageSet, PostError> {
        self.validate_config(device, targets.config())?;
        if let Some(post) = &self.post {
            Ok(post.build(
                device,
                targets,
                slot,
                self.format,
                (self.exposure, self.tone_map),
            ))
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
        self.validate_config(device, targets.config())?;
        if let Some(post) = &self.post {
            return Ok(post.build(
                device,
                targets,
                0,
                self.format,
                (self.exposure, self.tone_map),
            ));
        }
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fixed exposure and display curve"),
            source: wgpu::ShaderSource::Wgsl(crate::aces::shader(include_str!("tone_map.wgsl"))),
        });
        let layout = output_layout(device);
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
                constants: &[
                    ("EXPOSURE", self.exposure),
                    ("CURVE", self.tone_map.constant()),
                ],
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
            stages: vec![Stage {
                label: "fixed exposure and tone mapping",
                ..Stage::fullscreen(
                    crate::StageId::new("tone", 0),
                    &pipeline,
                    &[&group],
                    StageTarget::Output,
                )
            }],
            resources: Resources::default(),
        })
    }
}

fn output_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    })
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
            ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Filmic).is_ok()
        );
    }
}
