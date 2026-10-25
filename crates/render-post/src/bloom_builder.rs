use crate::{Bloom, BrightExtraction, GaussianLevel, HdrPostSettings, PostError, StageSet};

/// Appends the existing separable bloom pyramid to a caller-owned schedule.
#[derive(Clone, Copy, Debug)]
pub struct BloomPyramid {
    pub bloom: Bloom,
    pub levels: [GaussianLevel; 8],
}
impl BloomPyramid {
    /// The returned HDR view contains folded bloom before final `Bloom::intensity`.
    /// Exposure is a STORAGE buffer whose first f32 is the current nonnegative exposure.
    /// The caller orders updates before these stages and retains ownership of exposure.
    pub fn append(
        self,
        device: &wgpu::Device,
        set: &mut StageSet,
        source: &wgpu::TextureView,
        exposure: Option<&wgpu::Buffer>,
    ) -> Result<wgpu::TextureView, PostError> {
        HdrPostSettings {
            bloom: Some(self.bloom),
            bloom_kernel: crate::BloomKernel::SeparablePyramid(self.levels),
            ..HdrPostSettings::default()
        }
        .validate()?;
        validate_source(device, source)?;
        crate::gaussian_extents(
            [source.texture().width(), source.texture().height()],
            self.bloom.levels,
        )?;
        if matches!(
            self.bloom.policies.extraction,
            BrightExtraction::ToneCurve(_)
        ) && exposure.is_none_or(|buffer| {
            buffer.size() < 4 || !buffer.usage().contains(wgpu::BufferUsages::STORAGE)
        }) {
            return Err(PostError::UnsupportedProfile(
                "bloom requires caller exposure storage",
            ));
        }
        Ok(
            crate::gaussian_gpu::GaussianPyramid::new(device, set).build(
                source,
                self.bloom,
                &self.levels,
                exposure.map(|buffer| (buffer, 1.0)),
            ),
        )
    }
}

fn validate_source(device: &wgpu::Device, source: &wgpu::TextureView) -> Result<(), PostError> {
    let texture = source.texture();
    if texture.dimension() != wgpu::TextureDimension::D2
        || texture.sample_count() != 1
        || texture.depth_or_array_layers() != 1
        || texture.mip_level_count() != 1
        || !texture
            .usage()
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        || !matches!(
            texture.format().sample_type(None, Some(device.features())),
            Some(wgpu::TextureSampleType::Float { filterable: true })
        )
    {
        return Err(PostError::UnsupportedProfile(
            "bloom source must be single-level filterable D2 color",
        ));
    }
    Ok(())
}

pub(crate) fn bright_constants(bloom: Bloom) -> Vec<(&'static str, f64)> {
    let mut constants = vec![
        ("THRESHOLD", bloom.threshold.into()),
        ("KNEE", bloom.knee.into()),
        (
            "QUARTER",
            f64::from(u8::from(
                bloom.policies.reduction == crate::BloomReduction::QuarterTexelAfterAverage,
            )),
        ),
    ];
    if let BrightExtraction::ToneCurve(p) = bloom.policies.extraction {
        constants.extend([
            ("MAXIMUM", p.maximum.into()),
            ("LUM_R", p.luminance[0].into()),
            ("LUM_G", p.luminance[1].into()),
            ("LUM_B", p.luminance[2].into()),
            ("EXPOSURE_SCALE", p.exposure_scale.into()),
            ("CONTRAST", p.contrast.into()),
            ("EPSILON", p.epsilon.into()),
            (
                "REINHARD",
                f64::from(u8::from(p.curve == crate::BrightCurve::Reinhard)),
            ),
        ]);
    }
    constants
}
