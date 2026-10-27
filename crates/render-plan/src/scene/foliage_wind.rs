use super::material_features::bounded;
use super::{SceneDirection, SceneValidationError};

/// Spatial input for the two oscillators. Positions use the scene's Z-up basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneWindPhase {
    WorldPosition,
    InstancePosition,
    /// Uses the existing per-vertex occlusion scalar as an authored phase seed.
    VertexOcclusion,
}

/// A sinusoidal perturbation around one. Frequency is microradians/second,
/// perturbation is millionths, and spatial phase is microradians/metre.
/// Frequencies span `2..=64_000_000` so integer millisecond periods fit u32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWindBand {
    frequency: u32,
    perturb: u32,
    spatial_phase: u32,
}
impl SceneWindBand {
    pub fn new(
        frequency: u32,
        perturb: u32,
        spatial_phase: u32,
    ) -> Result<Self, SceneValidationError> {
        if !(2..=64_000_000).contains(&frequency) {
            return Err(SceneValidationError::RangeViolation {
                part: "wind band frequency",
                minimum: 2,
                maximum: 64_000_000,
                observed: u64::from(frequency),
            });
        }
        bounded("wind perturbation", perturb, 1_000_000)?;
        bounded("wind spatial phase", spatial_phase, 64_000_000)?;
        Ok(Self {
            frequency,
            perturb,
            spatial_phase,
        })
    }
    pub const fn angular_frequency_micro(self) -> u32 {
        self.frequency
    }
    pub const fn perturb_micro(self) -> u32 {
        self.perturb
    }
    pub const fn spatial_phase_micro(self) -> u32 {
        self.spatial_phase
    }
}

/// Arc generation at zero and small force. Existing descriptors use the straight limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneWindArc {
    StraightLimit,
    /// Clamps each stage's extent arc angle to at least half a degree.
    MinimumHalfDegree,
}

/// Two-stage authored-basis axial and radial bending. Extents are world-space
/// millimetres; the normalized local axis follows the original instance transform.
/// The arc basis is not orthogonalized, so length is preserved only for
/// perpendicular force. Oblique and parallel forces can stretch the segment.
/// Curve one gives constant curvature in this basis. Larger powers stiffen the base.
/// The primary and radial bands modulate around one, rather than reversing wind.
///
/// Wind uses the instance basis for the axis and local hinge even on billboards,
/// whose displayed geometry uses a camera-facing basis. Billboard base rings
/// therefore need not stay fixed; the world-space hinge itself remains fixed.
/// Normals and stored tangents retain their undeformed geometry frame in forward
/// and prepass shading, including subsequent terrain-normal folding.
///
/// By default zero force is the straight-segment limit and leaves geometry
/// unchanged. The optional minimum arc also applies at zero force.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneFoliageWind {
    arc: SceneWindArc,
    axis: SceneDirection,
    extents_millimeters: [u32; 2],
    curve_micro: u32,
    bands: [SceneWindBand; 2],
    phase: SceneWindPhase,
    strength_micro: u32,
    origin_millimeters: [i32; 3],
}
impl SceneFoliageWind {
    pub fn new(
        axis: SceneDirection,
        extents_millimeters: [u32; 2],
        curve_micro: u32,
        bands: [SceneWindBand; 2],
        phase: SceneWindPhase,
    ) -> Result<Self, SceneValidationError> {
        if extents_millimeters.contains(&0) {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "foliage extent",
            });
        }
        if !(1_000_000..=8_000_000).contains(&curve_micro) {
            return Err(SceneValidationError::RangeViolation {
                part: "wind curve",
                minimum: 1_000_000,
                maximum: 8_000_000,
                observed: u64::from(curve_micro),
            });
        }
        Ok(Self {
            arc: SceneWindArc::StraightLimit,
            axis,
            extents_millimeters,
            curve_micro,
            bands,
            phase,
            strength_micro: 1_000_000,
            origin_millimeters: [0; 3],
        })
    }
    #[must_use]
    pub const fn with_arc(mut self, arc: SceneWindArc) -> Self {
        self.arc = arc;
        self
    }
    pub const fn arc(self) -> SceneWindArc {
        self.arc
    }
    /// Moves the hinge in local mesh coordinates before instance transformation.
    #[must_use]
    pub const fn with_origin(mut self, millimeters: [i32; 3]) -> Self {
        self.origin_millimeters = millimeters;
        self
    }
    pub const fn origin_millimeters(self) -> [i32; 3] {
        self.origin_millimeters
    }
    pub fn with_strength(mut self, micro: u32) -> Result<Self, SceneValidationError> {
        bounded("wind strength", micro, 4_000_000)?;
        self.strength_micro = micro;
        Ok(self)
    }
    pub const fn axis(self) -> SceneDirection {
        self.axis
    }
    pub const fn extents_millimeters(self) -> [u32; 2] {
        self.extents_millimeters
    }
    pub const fn curve_micro(self) -> u32 {
        self.curve_micro
    }
    pub const fn bands(self) -> [SceneWindBand; 2] {
        self.bands
    }
    pub const fn phase(self) -> SceneWindPhase {
        self.phase
    }
    pub const fn strength_micro(self) -> u32 {
        self.strength_micro
    }
}
