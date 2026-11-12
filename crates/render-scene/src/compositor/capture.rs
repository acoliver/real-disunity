use super::FrameRenderer;

/// Existing single-sample frame products. Reading these never enables a feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureTarget {
    /// Positive pixel-centre view-space metres, before water, `R32Float`.
    Depth,
    /// View-space XYZ normals with coverage in alpha, `Rgba16Float`.
    Normals,
    /// Post-decal pixel-centre SSR material plane: linear F0 RGB and the stored
    /// material roughness channel in alpha, before lighting remap or lower clamp.
    /// With `SceneRoughnessRemap::LinearAlpha`, alpha is the authored GGX alpha
    /// channel, not perceptual roughness. Before water; single-sample `Rgba16Float`.
    /// Available when SSR metadata is enabled.
    Specular,
    /// Filtered visibility in red. Nearest uses `R32Float`, or `Rg16Float` for
    /// Scalable with normalized depth in green. `DepthAwareGather` uses `Rg32Float`
    /// with the consumed view-space metre depth in green, at the kernel's extent.
    AmbientOcclusion,
    /// Strict-facing trace confidence from the bank that opaque receivers read,
    /// in alpha of `Rgba16Float`, at trace resolution. Capture extent is
    /// `(trace_width * frame_sample_count, trace_height)`. Output samples occupy
    /// horizontal lanes: `packed_x = sample * trace_width + x`. Trace dimensions
    /// follow the selected resolution and rounding policy. Use `feature_capture`
    /// for the logical extent and physical lane layout, excluding allocation padding.
    SsrConfidence,
    /// Water-facing trace confidence in alpha of `Rgba16Float`, at trace resolution.
    /// Available only when the water-facing bank is enabled.
    SsrConfidenceWater,
}

/// Logical capture rectangle and sample-lane layout in an existing allocation.
#[derive(Clone, Copy)]
pub struct FeatureCapture<'a> {
    pub texture: &'a wgpu::Texture,
    /// Output extent, with all samples arranged horizontally.
    pub size: [u32; 2],
    pub lanes: u32,
    pub columns: u32,
    /// Distance between neighbouring lane origins in the backing texture.
    pub stride: [u32; 2],
    /// Array layer to capture.
    pub layer: u32,
}
impl<'a> FeatureCapture<'a> {
    pub fn whole(texture: &'a wgpu::Texture) -> Self {
        let size = [texture.width(), texture.height()];
        Self {
            texture,
            size,
            lanes: 1,
            columns: 1,
            stride: size,
            layer: 0,
        }
    }
}
impl FrameRenderer {
    /// Borrows a backing allocation. Its dimensions can include storage padding.
    pub fn feature_target(&self, feature: FeatureTarget) -> Option<&wgpu::Texture> {
        match feature {
            FeatureTarget::Depth => self.ssao.as_ref().map(|s| s.passes.depth_levels()[0]),
            FeatureTarget::Normals => self.ssao.as_ref().map(|s| &s.normals),
            FeatureTarget::AmbientOcclusion => self.ssao.as_ref().map(|s| &s.passes.filtered),
            FeatureTarget::Specular => self.ssr.as_ref().map(crate::ssr::Ssr::material_target),
            FeatureTarget::SsrConfidence => {
                self.ssr.as_ref().map(crate::ssr::Ssr::confidence_target)
            }
            FeatureTarget::SsrConfidenceWater => self
                .ssr
                .as_ref()
                .filter(|ssr| {
                    self.water.is_some()
                        && ssr.settings.hit_facing
                            == worldrunner_render_plan::SceneSsrHitFacing::AnyForWaterReceivers
                })
                .map(crate::ssr::Ssr::confidence_target),
        }
    }

    pub fn feature_capture(&self, feature: FeatureTarget) -> Option<FeatureCapture<'_>> {
        match feature {
            FeatureTarget::SsrConfidence => self.ssr.as_ref().map(|ssr| ssr.confidence_capture(0)),
            FeatureTarget::SsrConfidenceWater => self
                .ssr
                .as_ref()
                .filter(|ssr| {
                    self.water.is_some()
                        && ssr.settings.hit_facing
                            == worldrunner_render_plan::SceneSsrHitFacing::AnyForWaterReceivers
                })
                .map(|ssr| ssr.confidence_capture(1)),
            _ => self.feature_target(feature).map(FeatureCapture::whole),
        }
    }
}
