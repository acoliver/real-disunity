use super::{SceneValidationError, material_features::bounded};

/// Direction parameterization of a rectangular image atlas in scene Z-up space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneImpostorLayout {
    /// Periodic azimuth columns and elevation rows from horizon to zenith.
    AzimuthElevation,
    /// Upper-hemisphere octahedral projection with triangular three-frame blending.
    /// The view's vertical component is clamped to 0.0001 scene metres before
    /// projection. Views below the object therefore approximate a horizon view.
    HemisphericalOctahedral,
}

/// Multi-view card approximation. All existing material maps share this layout.
/// Atlas cells include an inset border, expressed in millionths of a cell.
/// The world eye is clamped at least 0.01 metres above the instance origin
/// for octahedral sampling. A zenith capture fixes its otherwise free horizontal
/// axis to +X.
/// Object rotation rotates the view into capture space (positive yaw rotates
/// the view counterclockwise); object scale supplies radius.
/// Octahedral images use virtual-plane projection; azimuth images use card UVs.
/// Sampling uses mip zero and a half-texel minimum inset to prevent cell bleeding.
/// Optional relief uses a linear depth atlas and equal-area object-space normals.
/// Relief affects depth, lighting and terrain contact in both forward shading
/// and metadata. Culling includes its view-ray displacement. Directional
/// shadows use the flat card's alpha cutout without relief depth displacement.
/// Decoded normals use the inverse of the full placement rotation used for
/// capture selection, including pitch and roll.
/// Optional border maps hold left, right, top and bottom crop fractions per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneImpostor {
    cutoffs_micro: Option<[u32; 2]>,
    channels: SceneImpostorChannels,
    sampling: SceneAtlasSampling,
    capture_center_millimeters: [i32; 3],
    fade: Option<SceneFadeBand>,
    relief: Option<(super::TextureHandle, u32)>,
    borders: Option<super::TextureHandle>,
    layout: SceneImpostorLayout,
    frames: [u32; 2],
    border_micro: u32,
    blend: bool,
    direction_distance_millimeters: u32,
    plane_size_millimeters: [u32; 2],
}
/// Atlas channel contract. The packed variant uses RT.r for roughness,
/// RT.g for metalness and translucency classification (> 0.05), and SIAO.rg
/// for emission and occlusion. RT/SIAO use the first selected capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneImpostorChannels {
    Material,
    RoughTranslucencyEmissionOcclusion,
}

/// Mip-zero sampling or the projected atlas UV's implicit derivative footprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneAtlasSampling {
    MipZero,
    Implicit,
}

impl SceneImpostor {
    pub(super) fn validate_channels(
        self,
        material: &super::SceneMaterial,
    ) -> Result<(), SceneValidationError> {
        if self.channels == SceneImpostorChannels::RoughTranslucencyEmissionOcclusion
            && (material.roughness_metallic().is_none() || material.ambient_occlusion().is_none())
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "packed impostor requires RT and emission-occlusion atlases",
            });
        }
        Ok(())
    }

    /// Overrides forward/metadata/prepass and shadow alpha cutoffs respectively.
    pub fn with_pass_cutoffs(
        mut self,
        forward: u32,
        shadow: u32,
    ) -> Result<Self, SceneValidationError> {
        bounded("impostor forward cutoff", forward, 1_000_000)?;
        bounded("impostor shadow cutoff", shadow, 1_000_000)?;
        self.cutoffs_micro = Some([forward, shadow]);
        Ok(self)
    }
    pub const fn pass_cutoffs_micro(self) -> Option<[u32; 2]> {
        self.cutoffs_micro
    }
    #[must_use]
    pub const fn with_channels(mut self, channels: SceneImpostorChannels) -> Self {
        self.channels = channels;
        self
    }
    pub const fn channels(self) -> SceneImpostorChannels {
        self.channels
    }
    #[must_use]
    pub const fn with_sampling(mut self, sampling: SceneAtlasSampling) -> Self {
        self.sampling = sampling;
        self
    }
    pub const fn sampling(self) -> SceneAtlasSampling {
        self.sampling
    }
    /// Mesh-local capture centre, transformed with the placement before wind.
    #[must_use]
    pub const fn with_capture_center(mut self, center: [i32; 3]) -> Self {
        self.capture_center_millimeters = center;
        self
    }
    pub const fn capture_center_millimeters(self) -> [i32; 3] {
        self.capture_center_millimeters
    }

    pub fn new(
        layout: SceneImpostorLayout,
        frames: [u32; 2],
        border_micro: u32,
        blend: bool,
    ) -> Result<Self, SceneValidationError> {
        if frames.contains(&0) {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "impostor frames",
            });
        }
        for count in frames {
            bounded("impostor frames", count, 256)?;
        }
        if layout == SceneImpostorLayout::HemisphericalOctahedral
            && (frames[0] < 2 || frames[0] != frames[1])
        {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "square octahedral grid with at least two frames",
            });
        }
        bounded("impostor border", border_micro, 499_999)?;
        Ok(Self {
            cutoffs_micro: None,
            channels: SceneImpostorChannels::Material,
            sampling: SceneAtlasSampling::MipZero,
            capture_center_millimeters: [0; 3],
            fade: None,
            relief: None,
            borders: None,
            layout,
            frames,
            border_micro,
            blend,
            direction_distance_millimeters: 0,
            plane_size_millimeters: [1000; 2],
        })
    }
    /// Object-centre distance fade-in, shared by forward and shadow coverage.
    #[must_use]
    pub const fn with_fade(mut self, band: SceneFadeBand) -> Self {
        self.fade = Some(band);
        self
    }
    pub const fn fade(self) -> Option<SceneFadeBand> {
        self.fade
    }
    /// Enables equal-area object-space RG normals and relief depth from the red
    /// channel of a linear atlas. One moves toward the camera by
    /// `scale_millimeters` in view depth; zero stays on the card plane.
    /// Decode precedes frame blending. Invalid sphere encodings clamp the root.
    pub fn with_relief(
        mut self,
        depth: super::TextureHandle,
        scale_millimeters: u32,
    ) -> Result<Self, SceneValidationError> {
        if scale_millimeters == 0 {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "impostor relief scale",
            });
        }
        self.relief = Some((depth, scale_millimeters));
        Ok(self)
    }
    /// One RGBA texel per frame, in left/right/top/bottom order. Each fraction
    /// removes that proportion of the corresponding half-plane. Blended frames
    /// use the componentwise minimum, preserving the union of their silhouettes.
    #[must_use]
    pub const fn with_borders(mut self, texture: super::TextureHandle) -> Self {
        self.borders = Some(texture);
        self
    }
    pub const fn relief(self) -> Option<(super::TextureHandle, u32)> {
        self.relief
    }
    pub const fn borders(self) -> Option<super::TextureHandle> {
        self.borders
    }
    /// Enables distance-dependent snapping using the quadratic approximation
    /// `(clamp(distance / threshold, 0, 1) - 0.5)^2`. Its weight is 0.25
    /// at both zero distance and beyond the threshold, and zero halfway through.
    /// Zero disables this approximation.
    #[must_use]
    pub const fn with_direction_distance(mut self, millimeters: u32) -> Self {
        self.direction_distance_millimeters = millimeters;
        self
    }
    /// Width and height of each captured plane before instance scale, in millimetres.
    /// Defaults to one metre square. The capture center is the instance origin.
    pub fn with_plane_size(mut self, size: [u32; 2]) -> Result<Self, SceneValidationError> {
        if size.contains(&0) {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "impostor plane",
            });
        }
        self.plane_size_millimeters = size;
        Ok(self)
    }
    pub const fn plane_size_millimeters(self) -> [u32; 2] {
        self.plane_size_millimeters
    }
    pub const fn layout(self) -> SceneImpostorLayout {
        self.layout
    }
    pub const fn frames(self) -> [u32; 2] {
        self.frames
    }
    pub const fn border_micro(self) -> u32 {
        self.border_micro
    }
    pub const fn blend(self) -> bool {
        self.blend
    }
    pub const fn direction_distance_millimeters(self) -> u32 {
        self.direction_distance_millimeters
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_rejects_invalid_cells_and_border() {
        use SceneImpostorLayout::{AzimuthElevation as A, HemisphericalOctahedral as O};
        for (layout, frames, border) in [
            (A, [0, 1], 0),
            (A, [257, 1], 0),
            (O, [1, 1], 0),
            (O, [2, 3], 0),
            (A, [4, 1], 500_000),
        ] {
            assert!(SceneImpostor::new(layout, frames, border, true).is_err());
        }
        assert!(SceneImpostor::new(A, [1, 1], 499_999, false).is_ok());
        let value = SceneImpostor::new(O, [8, 8], 0, true)
            .unwrap()
            .with_direction_distance(500_000);
        assert_eq!(value.frames(), [8, 8]);
        assert_eq!(value.direction_distance_millimeters(), 500_000);
    }
}

/// Object-centre distance fade-in. Below start the object is absent; at end it
/// is fully covered. Widths below one metre retain a one-metre denominator,
/// followed by full coverage above end. Distances are measured before wind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneFadeBand {
    terminal: bool,
    start_millimeters: u32,
    end_millimeters: u32,
}
impl SceneFadeBand {
    pub fn new(start_millimeters: u32, end_millimeters: u32) -> Result<Self, SceneValidationError> {
        if end_millimeters <= start_millimeters {
            return Err(SceneValidationError::ExtentNotPositive { part: "fade width" });
        }
        Ok(Self {
            terminal: false,
            start_millimeters,
            end_millimeters,
        })
    }
    /// Terminal coverage is one through start, then falls to zero at end.
    #[must_use]
    pub const fn with_terminal_fade(mut self) -> Self {
        self.terminal = true;
        self
    }
    pub const fn terminal(self) -> bool {
        self.terminal
    }
    pub const fn millimeters(self) -> [u32; 2] {
        [self.start_millimeters, self.end_millimeters]
    }
}
