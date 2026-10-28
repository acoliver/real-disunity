use super::FrameRenderer;

/// Existing single-sample frame products. Reading these never enables a feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureTarget {
    /// Positive pixel-centre view-space metres, before water, `R32Float`.
    Depth,
    /// View-space XYZ normals with coverage in alpha, `Rgba16Float`.
    Normals,
    /// Filtered visibility in red, `R32Float` or `Rg16Float` (green is filter metadata).
    AmbientOcclusion,
    /// Trace confidence in alpha of `Rgba16Float`, at trace resolution.
    /// Capture extent is `(trace_width * frame_sample_count, trace_height)`.
    /// Output samples occupy horizontal lanes: `packed_x = sample * trace_width + x`.
    /// Trace dimensions follow the selected resolution and rounding policy.
    /// Use `feature_capture` for the logical extent and physical lane layout,
    /// excluding allocation padding.
    SsrConfidence,
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
            FeatureTarget::SsrConfidence => {
                self.ssr.as_ref().map(crate::ssr::Ssr::confidence_target)
            }
        }
    }

    pub fn feature_capture(&self, feature: FeatureTarget) -> Option<FeatureCapture<'_>> {
        if feature == FeatureTarget::SsrConfidence {
            return self.ssr.as_ref().map(crate::ssr::Ssr::confidence_capture);
        }
        self.feature_target(feature).map(FeatureCapture::whole)
    }
}
