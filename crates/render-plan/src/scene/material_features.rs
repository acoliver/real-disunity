//! Optional material inputs in scene units, independent of content storage.
use super::{BillboardPolicy, MaterialHandle, SceneDirection, SceneValidationError};

pub(super) fn bounded(
    part: &'static str,
    value: u32,
    maximum: u32,
) -> Result<(), SceneValidationError> {
    if value > maximum {
        return Err(SceneValidationError::RangeViolation {
            part,
            minimum: 0,
            maximum: u64::from(maximum),
            observed: u64::from(value),
        });
    }
    Ok(())
}

/// Linear RGB multiplier, quantized to millionths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTint([u32; 3]);
impl SceneTint {
    pub const WHITE: Self = Self([1_000_000; 3]);
    pub fn new(channels: [u32; 3]) -> Result<Self, SceneValidationError> {
        for value in channels {
            bounded("tint channel", value, 1_000_000)?;
        }
        Ok(Self(channels))
    }
    pub const fn channels(self) -> [u32; 3] {
        self.0
    }
}

/// Baked occlusion multiplier and optional vertex-occlusion participation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneAmbientOcclusion {
    factor_micro: u32,
    vertex: bool,
}
impl SceneAmbientOcclusion {
    pub const UNOCCLUDED: Self = Self {
        factor_micro: 1_000_000,
        vertex: false,
    };
    pub fn new(factor_micro: u32, vertex: bool) -> Result<Self, SceneValidationError> {
        bounded("ambient occlusion factor", factor_micro, 1_000_000)?;
        Ok(Self {
            factor_micro,
            vertex,
        })
    }
    pub const fn factor_micro(self) -> u32 {
        self.factor_micro
    }
    pub const fn vertex(self) -> bool {
        self.vertex
    }
}

/// Closed interpolation policy for a second material in the same resource table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneLayerMode {
    /// Interpolate all material channels with one unit weight.
    Linear,
    /// Height-aware interpolation; requires a height-capable rendering implementation.
    Height,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneLayerBlend {
    material: MaterialHandle,
    weight_micro: u32,
    mode: SceneLayerMode,
}
impl SceneLayerBlend {
    pub fn new(
        material: MaterialHandle,
        weight_micro: u32,
        mode: SceneLayerMode,
    ) -> Result<Self, SceneValidationError> {
        bounded("layer weight", weight_micro, 1_000_000)?;
        Ok(Self {
            material,
            weight_micro,
            mode,
        })
    }
    pub const fn material(self) -> MaterialHandle {
        self.material
    }
    pub const fn weight_micro(self) -> u32 {
        self.weight_micro
    }
    pub const fn mode(self) -> SceneLayerMode {
        self.mode
    }
}

/// Periodic world-space displacement. Direction is already in the scene Z-up basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWind {
    direction: SceneDirection,
    amplitude_millimeters: u32,
    period_milliseconds: u32,
    phase_micro: u32,
}
impl SceneWind {
    pub fn new(
        direction: SceneDirection,
        amplitude_millimeters: u32,
        period_milliseconds: u32,
        phase_micro: u32,
    ) -> Result<Self, SceneValidationError> {
        if period_milliseconds == 0 {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "wind period",
            });
        }
        bounded("wind phase", phase_micro, 999_999)?;
        Ok(Self {
            direction,
            amplitude_millimeters,
            period_milliseconds,
            phase_micro,
        })
    }
    pub const fn direction(self) -> SceneDirection {
        self.direction
    }
    pub const fn amplitude_millimeters(self) -> u32 {
        self.amplitude_millimeters
    }
    pub const fn period_milliseconds(self) -> u32 {
        self.period_milliseconds
    }
    pub const fn phase_micro(self) -> u32 {
        self.phase_micro
    }
}

/// Card dimensions and transition coverage for an existing billboard LOD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneCard {
    policy: BillboardPolicy,
    size_millimeters: [u32; 2],
    coverage_micro: u32,
}
impl SceneCard {
    pub fn new(
        policy: BillboardPolicy,
        size_millimeters: [u32; 2],
        coverage_micro: u32,
    ) -> Result<Self, SceneValidationError> {
        if size_millimeters.contains(&0) {
            return Err(SceneValidationError::ExtentNotPositive { part: "card size" });
        }
        bounded("card coverage", coverage_micro, 1_000_000)?;
        Ok(Self {
            policy,
            size_millimeters,
            coverage_micro,
        })
    }
    pub const fn policy(self) -> BillboardPolicy {
        self.policy
    }
    pub const fn size_millimeters(self) -> [u32; 2] {
        self.size_millimeters
    }
    pub const fn coverage_micro(self) -> u32 {
        self.coverage_micro
    }
}

/// Optional authored material inputs. Absence preserves stock shading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneMaterialFeatures {
    pub tint: SceneTint,
    pub ambient_occlusion: SceneAmbientOcclusion,
    pub wind: Option<SceneWind>,
    pub layer: Option<SceneLayerBlend>,
}
