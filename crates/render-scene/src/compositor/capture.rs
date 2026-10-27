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
    /// The native packed extent is `(trace_width * frame_sample_count, trace_height)`.
    /// Samples occupy contiguous horizontal lanes: `packed_x = sample * trace_width + x`,
    /// where `trace_width = returned_width / frame_sample_count`.
    /// Trace dimensions match the raster extent at full resolution, or its
    /// ceil-divided halves at half resolution; the readback does not rearrange samples.
    SsrConfidence,
}
impl FrameRenderer {
    /// Borrows a target already produced by this frame configuration.
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
}
