//! The disocclusion test of the f64 reference: history is dropped where the surface now visible
//! was not the one stored in history. See `taa.wgsl` for the GPU version.
use super::Frame;

/// Largest relative difference between the depth a history sample is predicted at and the
/// depth stored for it before the sample is judged to belong to another surface.
pub(super) const DEPTH_TOLERANCE: f64 = 0.02;

/// Where the history for a pixel lies and what depth the surface there should have.
pub(super) struct Lookup {
    pub(super) uv: [f64; 2],
    /// Linear depth that texel had in the previous frame: the reprojection maps NDC, so its `w`
    /// is the previous view depth divided by the current one.
    pub(super) predicted_depth: f64,
    /// No object-motion residual: the camera reprojection alone moved the texel.
    pub(super) camera_only: bool,
}

impl Frame<'_> {
    /// Linear view depth of device depth `z`, or 0 when the depth terms are unset.
    pub(super) fn linear_depth(&self, z: f64) -> f64 {
        let [a, b] = self.depth_terms;
        b / (z.min(1.0) - a)
    }

    /// The linear depth the resolve stores for a pixel: that of its closest-depth texel.
    pub fn stored_depth(&self, at: [i32; 2]) -> f64 {
        let q = self.closest(at);
        self.linear_depth(self.depth.load(q[0], q[1]))
    }

    /// Whether the surface now visible is not the one in history. Only static or camera-only
    /// pixels are tested: the residual is XY, so a moving object that changes view depth would
    /// fail falsely. The stored depth over the 2x2 bilinear footprint at `uv` spans an interval
    /// (wide where the surface is steep, as at grazing angles) and the prediction must lie in
    /// it, widened by [`DEPTH_TOLERANCE`].
    pub(super) fn disoccluded(&self, lookup: &Lookup) -> bool {
        let Some(stored) = self.depth_history else {
            return false;
        };
        let predicted = lookup.predicted_depth;
        if !lookup.camera_only || self.depth_terms[1] == 0.0 || !predicted.is_finite() {
            return false;
        }
        let size = stored.size.map(f64::from);
        let base = [
            (lookup.uv[0] * size[0] - 0.5).floor(),
            (lookup.uv[1] * size[1] - 0.5).floor(),
        ];
        #[expect(
            clippy::cast_possible_truncation,
            reason = "bounded by the image extent"
        )]
        let texels = (0..4).map(|i| stored.load(base[0] as i32 + i % 2, base[1] as i32 + i / 2));
        let (lo, hi) = texels.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
            (lo.min(d), hi.max(d))
        });
        predicted < lo * (1.0 - DEPTH_TOLERANCE) || predicted > hi * (1.0 + DEPTH_TOLERANCE)
    }
}
