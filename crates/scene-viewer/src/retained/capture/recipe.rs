use super::CaptureRequestError;

/// Deterministic sub-pixel offsets, in output-pixel units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureJitter {
    None,
    Halton23,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureOutput {
    /// Linear, post-composition opaque reflectance, pixel-centre single sample.
    Albedo,
    /// Strength-adjusted sun visibility in [0, 1], opaque receivers only.
    SunVisibility,
    /// Sun diffuse radiance after directional visibility and configured diffuse AO.
    DirectDiffuse,
    /// Hemisphere or SH diffuse radiance after material and screen AO.
    AmbientDiffuse,
    /// Environment specular radiance, before exposure, fog and SSR replacement.
    EnvironmentSpecular,
    /// Directional specular radiance, including the configured clipped sun disk.
    SunSpecular,
    Srgb8,
    LinearHdr,
    Depth,
    Normals,
    /// Post-decal pixel-centre SSR material plane: linear F0 RGB and the material
    /// roughness channel as stored for SSR, before lighting remap and lower clamp.
    /// Under `SceneRoughnessRemap::LinearAlpha`, alpha is the authored GGX alpha
    /// channel, not perceptual roughness. Before water; requires SSR.
    Specular,
    AmbientOcclusion,
    SsrConfidence,
}

/// Validated capture intent. Unsupported execution modes are rejected at admission.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedCaptureRecipe {
    pub(crate) supersample: u32,
    pub(crate) temporal_samples: u32,
    pub(crate) jitter: CaptureJitter,
    pub(crate) advance_clock: bool,
    pub(crate) outputs: Vec<CaptureOutput>,
    pub(crate) max_bytes: u64,
    pub(crate) require_pages: bool,
    pub(crate) require_geometry: bool,
    pub(crate) frame_samples: u32,
    pub(crate) ao_format: wgpu::TextureFormat,
    native_rcas: Option<crate::NativeRcasSettings>,
}
impl RetainedCaptureRecipe {
    pub(crate) fn set_ssao(&mut self, settings: Option<worldrunner_render_plan::SceneSsao>) {
        self.ao_format = super::ao_format_from_settings(settings);
    }

    pub fn new(
        supersample: u32,
        temporal_samples: u32,
        jitter: CaptureJitter,
        advance_clock: bool,
        outputs: Vec<CaptureOutput>,
        max_bytes: u64,
    ) -> Result<Self, CaptureRequestError> {
        if supersample == 0 || temporal_samples == 0 || outputs.is_empty() || max_bytes == 0 {
            return Err(CaptureRequestError::InvalidRecipe);
        }
        if outputs.iter().any(|output| output.term().is_some())
            && (supersample != 1
                || temporal_samples != 1
                || jitter != CaptureJitter::None
                || advance_clock)
        {
            return Err(CaptureRequestError::UnsupportedRecipe);
        }
        u64::from(supersample)
            .checked_pow(2)
            .and_then(|n| n.checked_mul(u64::from(temporal_samples)))
            .and_then(|n| n.checked_mul(255))
            .ok_or(CaptureRequestError::InvalidRecipe)?;
        for (index, output) in outputs.iter().enumerate() {
            if outputs[..index].contains(output) {
                return Err(CaptureRequestError::InvalidRecipe);
            }
        }
        Ok(Self {
            supersample,
            temporal_samples,
            jitter,
            advance_clock,
            outputs,
            max_bytes,
            require_pages: false,
            require_geometry: false,
            frame_samples: 1,
            ao_format: wgpu::TextureFormat::R32Float,
            native_rcas: None,
        })
    }
    /// Wait for current-view material pages using retained page-residency readiness.
    #[must_use]
    pub fn with_page_dependency(mut self) -> Self {
        self.require_pages = true;
        self
    }
    /// Wait for source-generation geometry at the requested raster footprint.
    #[must_use]
    pub fn with_geometry_dependency(mut self) -> Self {
        self.require_geometry = true;
        self
    }
    /// Select native RCAS for each full-resolution capture sample. Requires an HDR frame.
    pub fn with_native_rcas(
        mut self,
        settings: crate::NativeRcasSettings,
    ) -> Result<Self, CaptureRequestError> {
        if self.outputs.iter().any(|output| output.term().is_some()) {
            return Err(CaptureRequestError::UnsupportedRecipe);
        }
        settings
            .validate()
            .map_err(|_| CaptureRequestError::InvalidRecipe)?;
        self.native_rcas = Some(settings);
        Ok(self)
    }
    pub fn native_rcas(&self) -> Option<crate::NativeRcasSettings> {
        self.native_rcas
    }
    pub fn supersample(&self) -> u32 {
        self.supersample
    }
    pub fn temporal_samples(&self) -> u32 {
        self.temporal_samples
    }
    pub fn jitter(&self) -> CaptureJitter {
        self.jitter
    }
    pub fn advances_clock(&self) -> bool {
        self.advance_clock
    }
    pub fn outputs(&self) -> &[CaptureOutput] {
        &self.outputs
    }
    /// Centered Halton bases 2 and 3, indexed from one; offsets use output pixels.
    pub fn jitter_offset(&self, sample: u32) -> [f32; 2] {
        assert!(sample < self.temporal_samples);
        match self.jitter {
            CaptureJitter::None => [0.0; 2],
            CaptureJitter::Halton23 => [
                radical_inverse(sample + 1, 2) - 0.5,
                radical_inverse(sample + 1, 3) - 0.5,
            ],
        }
    }
    /// Advancing recipes use an explicit deterministic 60 Hz subframe clock.
    pub fn subframe_time(&self, base: std::time::Duration, sample: u32) -> std::time::Duration {
        assert!(sample < self.temporal_samples);
        base + if self.advance_clock {
            std::time::Duration::from_nanos(u64::from(sample) * 1_000_000_000 / 60)
        } else {
            std::time::Duration::ZERO
        }
    }
    pub(crate) fn accumulated(&self) -> bool {
        self.native_rcas.is_some()
            || self.supersample != 1
            || self.temporal_samples != 1
            || self.jitter != CaptureJitter::None
            || self.advance_clock
    }
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub(crate) fn render_size(&self, size: [u32; 2]) -> Result<[u32; 2], CaptureRequestError> {
        Ok([
            size[0]
                .checked_mul(self.supersample)
                .ok_or(CaptureRequestError::InvalidRecipe)?,
            size[1]
                .checked_mul(self.supersample)
                .ok_or(CaptureRequestError::InvalidRecipe)?,
        ])
    }

    pub(crate) const POOL_BYTES: u64 = 256 * 1024 * 1024;

    pub(crate) fn pool_bytes(&self, size: [u32; 2]) -> Result<u64, CaptureRequestError> {
        let required = self.storage_bytes(size)?;
        if required > Self::POOL_BYTES {
            return Err(CaptureRequestError::RecipeBudget {
                required,
                limit: Self::POOL_BYTES,
            });
        }
        Ok(required)
    }

    pub(crate) fn storage_bytes(&self, size: [u32; 2]) -> Result<u64, CaptureRequestError> {
        if size.contains(&0) {
            return Err(CaptureRequestError::InvalidRecipe);
        }
        let output_size = size;
        let size = self.render_size(size)?;
        let mut bytes = 0_u64;
        for output in &self.outputs {
            bytes = bytes
                .checked_add(output_storage(
                    *output,
                    size,
                    self.frame_samples,
                    self.ao_format,
                )?)
                .ok_or(CaptureRequestError::InvalidRecipe)?;
        }
        if self.accumulated() {
            let accumulation = u64::from(output_size[0])
                .checked_mul(u64::from(output_size[1]))
                .and_then(|n| n.checked_mul(48))
                .and_then(|n| {
                    n.checked_mul(u64::try_from(self.outputs.len()).expect("output count"))
                })
                .ok_or(CaptureRequestError::InvalidRecipe)?;
            let offscreen = u64::from(size[0]) * u64::from(size[1]) * 4;
            bytes = bytes
                .checked_add(offscreen)
                .and_then(|n| n.checked_add(accumulation))
                .ok_or(CaptureRequestError::InvalidRecipe)?;
        }
        if bytes > self.max_bytes {
            return Err(CaptureRequestError::RecipeBudget {
                required: bytes,
                limit: self.max_bytes,
            });
        }
        Ok(bytes)
    }
}

fn output_storage(
    output: CaptureOutput,
    size: [u32; 2],
    samples: u32,
    ao_format: wgpu::TextureFormat,
) -> Result<u64, CaptureRequestError> {
    let width = u64::from(size[0])
        .checked_mul(if output == CaptureOutput::SsrConfidence {
            u64::from(samples)
        } else {
            1
        })
        .ok_or(CaptureRequestError::InvalidRecipe)?;
    let (texel_bytes, cpu_bytes) = match output {
        CaptureOutput::Srgb8 => (4, 4),
        CaptureOutput::Specular => (8, 8),
        CaptureOutput::LinearHdr
        | CaptureOutput::Normals
        | CaptureOutput::Albedo
        | CaptureOutput::SunVisibility
        | CaptureOutput::DirectDiffuse
        | CaptureOutput::AmbientDiffuse
        | CaptureOutput::EnvironmentSpecular
        | CaptureOutput::SunSpecular => (8, 24),
        CaptureOutput::SsrConfidence => (8, 12),
        CaptureOutput::AmbientOcclusion if ao_format == wgpu::TextureFormat::Rg32Float => (8, 12),
        CaptureOutput::Depth | CaptureOutput::AmbientOcclusion => (4, 8),
    };
    let targets = if output.term().is_some() {
        worldrunner_render_scene::compositor::RendererConfiguration::capture_term_allocations(size)
            .validate(&wgpu::Limits::default())
            .map_err(|_| CaptureRequestError::InvalidRecipe)?
    } else {
        0
    };
    let row = width
        .checked_mul(texel_bytes)
        .ok_or(CaptureRequestError::InvalidRecipe)?;
    // Staging, mapped copy, tightly packed bytes and decoded pixels.
    row.next_multiple_of(256)
        .checked_mul(2)
        .and_then(|n| n.checked_add(width.checked_mul(cpu_bytes)?))
        .and_then(|n| n.checked_mul(u64::from(size[1])))
        .and_then(|n| n.checked_add(targets))
        .ok_or(CaptureRequestError::InvalidRecipe)
}

fn radical_inverse(mut index: u32, base: u32) -> f32 {
    let mut value = 0.0_f64;
    let mut weight = 1.0 / f64::from(base);
    while index != 0 {
        value += f64::from(index % base) * weight;
        index /= base;
        weight /= f64::from(base);
    }
    glam::DVec2::splat(value).as_vec2().x
}

/// Requested capture products. Feature targets retain their native raster extents.
/// Feature readbacks currently require one unjittered, non-accumulated frame.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RetainedCaptureProducts {
    pub(crate) terms: Option<Box<OpaqueCaptureProducts>>,
    pub srgb8: Option<worldrunner_render_scene::offscreen::RgbaImage>,
    pub linear_hdr: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    /// Positive pixel-centre view-space metres before water.
    pub depth: Option<worldrunner_render_scene::offscreen::ScalarImage>,
    /// View-space XYZ normals; alpha is coverage, zero for uncovered pixels.
    pub normals: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    /// Native raster-size `RGBA16Float`: linear F0 RGB and the material roughness
    /// channel as stored for SSR, after textures, contact surfaces and ordered
    /// projected decals, before the lighting roughness remap and lower clamp.
    /// Under `SceneRoughnessRemap::LinearAlpha`, alpha is the authored GGX alpha
    /// channel, not perceptual roughness; authored zero remains zero.
    /// F0 = mix(0.04, linear albedo, metalness). Uncovered pixels are (0, 0, 0, 1).
    /// This is the single-sample pixel-centre opaque/masked SSR receiver plane,
    /// before water, transparent surfaces, exposure, lighting and post-processing.
    /// Bytes are tightly packed little-endian half floats, top-left first, row-major,
    /// exactly width * height * 8 bytes. No MSAA lanes or coverage alpha.
    pub specular: Option<worldrunner_render_scene::offscreen::Rgba16FloatImage>,
    /// Filtered ambient visibility in [0, 1], decoded from the target red channel.
    pub ambient_occlusion: Option<worldrunner_render_scene::offscreen::ScalarImage>,
    /// Confidence in [0, 1] from trace alpha, at native trace resolution.
    /// The returned extent is `(trace_width * frame_sample_count, trace_height)`.
    /// Samples occupy contiguous horizontal lanes: `packed_x = sample * trace_width + x`,
    /// where `trace_width = returned_width / frame_sample_count` and the sample count
    /// comes from the frame profile. Trace dimensions match the raster extent at
    /// full resolution, or its ceil-divided halves at half resolution. Read pixel
    /// `(x, y)` of a sample at `pixels[y * returned_width + packed_x]`.
    pub ssr_confidence: Option<worldrunner_render_scene::offscreen::ScalarImage>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn specular_budget_accounts_for_native_bytes_without_msaa_lanes() {
        for samples in [1, 4] {
            // Two padded staging copies and one tightly packed raw plane.
            assert_eq!(
                output_storage(
                    CaptureOutput::Specular,
                    [33, 2],
                    samples,
                    wgpu::TextureFormat::R32Float
                ),
                Ok(2576)
            );
        }
        assert_eq!(
            output_storage(
                CaptureOutput::Specular,
                [u32::MAX; 2],
                1,
                wgpu::TextureFormat::R32Float
            ),
            Err(CaptureRequestError::InvalidRecipe)
        );
    }
    #[test]
    fn ao_native_format_budget_preserves_nearest_and_counts_gather_storage() {
        for format in [
            wgpu::TextureFormat::R32Float,
            wgpu::TextureFormat::Rg16Float,
        ] {
            assert_eq!(
                output_storage(CaptureOutput::AmbientOcclusion, [33, 2], 1, format),
                Ok(1552)
            );
        }
        assert_eq!(
            output_storage(
                CaptureOutput::AmbientOcclusion,
                [33, 2],
                1,
                wgpu::TextureFormat::Rg32Float
            ),
            Ok(2840)
        );
    }
    #[test]
    fn recipes_reject_empty_duplicate_and_over_budget_outputs() {
        let make = |outputs, max| {
            RetainedCaptureRecipe::new(1, 1, CaptureJitter::None, false, outputs, max)
        };
        assert!(make(vec![], 100).is_err());
        assert!(make(vec![CaptureOutput::Srgb8; 2], 100).is_err());
        let recipe = make(vec![CaptureOutput::LinearHdr], 1).unwrap();
        assert!(matches!(
            recipe.storage_bytes([64, 64]),
            Err(CaptureRequestError::RecipeBudget { .. })
        ));
        assert!(recipe.storage_bytes([u32::MAX, u32::MAX]).is_err());
    }
}

#[cfg(test)]
mod native_rcas_tests {
    use super::*;
    #[test]
    fn native_rcas_recipe_strength_is_validated_and_off_is_default() {
        let recipe = RetainedCaptureRecipe::new(
            1,
            1,
            CaptureJitter::None,
            false,
            vec![CaptureOutput::Srgb8],
            4096,
        )
        .unwrap();
        assert_eq!(recipe.native_rcas(), None);
        assert!(!recipe.accumulated());
        for stops in [-0.01, 2.01, f32::NAN, f32::NEG_INFINITY] {
            assert_eq!(
                recipe.clone().with_native_rcas(crate::NativeRcasSettings {
                    stops,
                    domain: crate::RcasDomain::EncodedOutput,
                }),
                Err(CaptureRequestError::InvalidRecipe)
            );
        }
        for stops in [0.0, 0.75, 2.0] {
            let settings = crate::NativeRcasSettings {
                stops,
                domain: crate::RcasDomain::EncodedOutput,
            };
            let selected = recipe.clone().with_native_rcas(settings).unwrap();
            assert_eq!(selected.native_rcas(), Some(settings));
            assert!(selected.accumulated());
        }
    }
}

impl CaptureOutput {
    pub(crate) fn term(self) -> Option<worldrunner_render_scene::compositor::terms::CaptureTerm> {
        use worldrunner_render_scene::compositor::terms::CaptureTerm;
        match self {
            Self::Albedo => Some(CaptureTerm::Albedo),
            Self::DirectDiffuse => Some(CaptureTerm::DirectDiffuse),
            Self::AmbientDiffuse => Some(CaptureTerm::AmbientDiffuse),
            Self::EnvironmentSpecular => Some(CaptureTerm::EnvironmentSpecular),
            Self::SunSpecular => Some(CaptureTerm::SunSpecular),
            Self::SunVisibility => Some(CaptureTerm::SunVisibility),
            _ => None,
        }
    }
}

/// Request-only opaque pixel-centre products. No storage is created for ordinary captures.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct OpaqueCaptureProducts {
    pub albedo: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    pub direct_diffuse: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    pub ambient_diffuse: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    pub environment_specular: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    pub sun_specular: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
    pub sun_visibility: Option<worldrunner_render_scene::offscreen::LinearRgbaImage>,
}
impl RetainedCaptureProducts {
    /// Linear reflectance RGB after material composition; alpha is opaque coverage.
    pub fn albedo(&self) -> Option<&worldrunner_render_scene::offscreen::LinearRgbaImage> {
        self.terms.as_ref().and_then(|terms| terms.albedo.as_ref())
    }
    /// Strength-adjusted [0, 1] visibility replicated in RGB; alpha is opaque coverage.
    pub fn sun_visibility(&self) -> Option<&worldrunner_render_scene::offscreen::LinearRgbaImage> {
        self.terms
            .as_ref()
            .and_then(|terms| terms.sun_visibility.as_ref())
    }
    /// Linear pre-exposure opaque radiance; alpha is coverage. See the capture contract.
    pub fn direct_diffuse(&self) -> Option<&worldrunner_render_scene::offscreen::LinearRgbaImage> {
        self.terms
            .as_ref()
            .and_then(|terms| terms.direct_diffuse.as_ref())
    }
    /// Linear pre-exposure opaque radiance; alpha is coverage. See the capture contract.
    pub fn ambient_diffuse(&self) -> Option<&worldrunner_render_scene::offscreen::LinearRgbaImage> {
        self.terms
            .as_ref()
            .and_then(|terms| terms.ambient_diffuse.as_ref())
    }
    /// Linear pre-exposure opaque radiance; alpha is coverage. See the capture contract.
    pub fn environment_specular(
        &self,
    ) -> Option<&worldrunner_render_scene::offscreen::LinearRgbaImage> {
        self.terms
            .as_ref()
            .and_then(|terms| terms.environment_specular.as_ref())
    }
    /// Linear pre-exposure opaque radiance; alpha is coverage. See the capture contract.
    pub fn sun_specular(&self) -> Option<&worldrunner_render_scene::offscreen::LinearRgbaImage> {
        self.terms
            .as_ref()
            .and_then(|terms| terms.sun_specular.as_ref())
    }
    pub(crate) fn store_term(
        &mut self,
        output: CaptureOutput,
        image: worldrunner_render_scene::offscreen::LinearRgbaImage,
    ) {
        let terms = self
            .terms
            .get_or_insert_with(|| Box::new(OpaqueCaptureProducts::default()));
        match output {
            CaptureOutput::Albedo => terms.albedo = Some(image),
            CaptureOutput::DirectDiffuse => terms.direct_diffuse = Some(image),
            CaptureOutput::AmbientDiffuse => terms.ambient_diffuse = Some(image),
            CaptureOutput::EnvironmentSpecular => terms.environment_specular = Some(image),
            CaptureOutput::SunSpecular => terms.sun_specular = Some(image),
            CaptureOutput::SunVisibility => terms.sun_visibility = Some(image),
            _ => unreachable!("opaque term output"),
        }
    }
}
