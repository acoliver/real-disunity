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
    /// Largest supported linear brightening multiplier, in millionths.
    pub const MAX_CHANNEL_MICRO: u32 = 4_000_000;
    pub fn new(channels: [u32; 3]) -> Result<Self, SceneValidationError> {
        for value in channels {
            bounded("tint channel", value, Self::MAX_CHANNEL_MICRO)?;
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
    field: Option<super::ScenePlanarWindField>,
    direction: SceneDirection,
    amplitude_millimeters: u32,
    period_milliseconds: u32,
    phase_micro: u32,
    foliage: Option<super::SceneFoliageWind>,
}
impl SceneWind {
    /// Replaces uniform directional force with a planar field for curved foliage.
    #[must_use]
    pub const fn with_field(mut self, field: super::ScenePlanarWindField) -> Self {
        self.field = Some(field);
        self
    }
    pub const fn field(self) -> Option<super::ScenePlanarWindField> {
        self.field
    }

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
            field: None,
            direction,
            amplitude_millimeters,
            period_milliseconds,
            phase_micro,
            foliage: None,
        })
    }
    /// Opts into curved deformation. Its bands supply frequency; the rigid period
    /// is retained for the default mode. The phase offset applies to both bands.
    #[must_use]
    pub const fn with_foliage(mut self, foliage: super::SceneFoliageWind) -> Self {
        self.foliage = Some(foliage);
        self
    }
    pub const fn foliage(self) -> Option<super::SceneFoliageWind> {
        self.foliage
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
/// Dimensions fit the centered horizontal extent and foot-to-top height.
/// At a selected card level, coverage is a screen-pixel fraction; the preceding
/// mesh fills its complement wherever their rasterized silhouettes overlap.
/// Alpha cutouts and each representation's silhouette remain authored separately.
/// Distance thresholds remain inclusive and depend only on the main camera.
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
    pub translucency: Option<super::SceneTranslucency>,
}

/// Constant terrain-contact surface values in linear scene units.
/// `tint` supplies target linear albedo, not a multiplier. Roughness and metalness
/// interpolate toward the target without changing alpha. Normal folding is opt-in.
/// Weight is one at the rendered heightfield and zero one distance away, on
/// either side. There is no contact outside the heightfield extent.
///
/// Normal folding assumes the instance origin sits on the terrain and samples
/// the ground normal at that origin. Its height is the authored, unscaled local
/// Z above the mesh origin, before wind. Translation, rotation and scale do not
/// change fold weights, even when the instance is raised off the terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTerrainContact {
    tint: SceneTint,
    roughness_micro: u32,
    metalness_micro: u32,
    distance_millimeters: u32,
    normal_fold: Option<[u32; 2]>,
    fold_sampling: super::SceneFoldSampling,
}
impl SceneTerrainContact {
    /// Enables folding with height in millimetres and minimum mesh weight in millionths.
    /// The mesh weight is `w = clamp(max(0, local_z) / height + minimum / 1_000_000, 0, 1)`,
    /// so vertices below the mesh origin keep the additive minimum weight.
    /// The minimum is additive, including at contact, rather than a lower bound.
    /// The result is `ground_normal * (1 - w) + mapped_mesh_normal * w`, deliberately
    /// unnormalized for surface lighting, backlight and normal metadata. Reflection
    /// consumers normalize at their input boundary, before reflection direction
    /// and Fresnel evaluation, in both forward environment sampling and SSR.
    pub fn with_normal_fold(
        mut self,
        height: u32,
        minimum: u32,
    ) -> Result<Self, SceneValidationError> {
        if height == 0 {
            return Err(SceneValidationError::ExtentNotPositive {
                part: "normal fold height",
            });
        }
        bounded("normal fold minimum", minimum, 1_000_000)?;
        self.normal_fold = Some([height, minimum]);
        Ok(self)
    }
    #[must_use]
    pub const fn with_fold_sampling(mut self, sampling: super::SceneFoldSampling) -> Self {
        self.fold_sampling = sampling;
        self
    }
    pub const fn fold_sampling(self) -> super::SceneFoldSampling {
        self.fold_sampling
    }
    pub const fn normal_fold(self) -> Option<[u32; 2]> {
        self.normal_fold
    }

    pub const fn tint(self) -> SceneTint {
        self.tint
    }
    pub const fn roughness_micro(self) -> u32 {
        self.roughness_micro
    }
    pub const fn metalness_micro(self) -> u32 {
        self.metalness_micro
    }
    pub const fn distance_millimeters(self) -> u32 {
        self.distance_millimeters
    }

    /// A zero distance disables material contact while retaining any normal fold.
    pub fn new(
        tint: SceneTint,
        roughness_micro: u32,
        metalness_micro: u32,
        distance_millimeters: u32,
    ) -> Result<Self, SceneValidationError> {
        bounded("contact roughness", roughness_micro, 1_000_000)?;
        bounded("contact metalness", metalness_micro, 1_000_000)?;
        Ok(Self {
            tint,
            roughness_micro,
            metalness_micro,
            distance_millimeters,
            normal_fold: None,
            fold_sampling: super::SceneFoldSampling::InstanceOrigin,
        })
    }
}

/// Stored tangent-space normal channels. The default preserves existing XY decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneNormalEncoding {
    Rgb,
    TwoChannelXy,
}

/// Direction of the normal texture's green channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneNormalY {
    Up,
    Down,
}
