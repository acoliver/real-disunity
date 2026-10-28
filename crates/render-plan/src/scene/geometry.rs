//! Owned renderer-domain meshes, transforms, and ordered scene objects.
//!
//! Geometry is closed CPU state authored in whole millimeters and micro-unit
//! ratios; there are no paths, no defaults, and no borrowed publication
//! data. Level indices, triangle lists, and handle references are validated
//! by construction.

use crate::scene::error::SceneValidationError;
use crate::scene::resource::MaterialHandle;

/// Micro-unit scale shared by rotation components.
pub const ROTATION_MICRO_SCALE: i64 = 1_000_000;
/// Micro-unit scale squared, the normalization target of rotations.
pub const ROTATION_MICRO_SCALE_SQUARED: i128 =
    ROTATION_MICRO_SCALE as i128 * ROTATION_MICRO_SCALE as i128;

/// Largest admissible deviation of a rotation's squared norm from
/// [`ROTATION_MICRO_SCALE_SQUARED`].
///
/// A unit quaternion quantized to micro-units rounds each component by at most
/// one half unit. Writing each stored component as `v + e` with `|e| <= 1/2`
/// and `sum(v*v) == ROTATION_MICRO_SCALE_SQUARED`, the stored squared norm is
/// `ROTATION_MICRO_SCALE_SQUARED + 2*sum(v*e) + sum(e*e)`. For a unit
/// quaternion `sum(|v|) <= 2 * ROTATION_MICRO_SCALE`, so `|2*sum(v*e)| <=
/// 2 * ROTATION_MICRO_SCALE` and `sum(e*e) <= 1`, giving this bound.
pub const ROTATION_NORMALIZATION_TOLERANCE: i128 = 2 * ROTATION_MICRO_SCALE as i128 + 1;

/// Dense index of one mesh inside a [`crate::scene::RenderScene`] mesh table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshHandle(u32);

impl MeshHandle {
    /// Adopts one raw table index.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw table index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// One owned vertex in canonical renderer-domain units.
///
/// The position is whole millimeters under the scene basis; the normal and
/// texture coordinates are micro-unit ratios the renderer interprets without
/// re-scaling. No per-vertex invariant exists beyond the types themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneVertex {
    /// Optional tangent XYZ in micro-units and handedness W, exactly `+/-1_000_000`.
    pub tangent_micro: Option<[i64; 4]>,
    /// Canonical position in whole millimeters.
    pub position_millimeters: [i64; 3],
    /// Authored surface direction in micro-units.
    pub normal_micro: [i64; 3],
    /// Authored texture coordinates in micro-units.
    pub texcoord_micro: [i64; 2],
}

/// Closed authored billboard policy of one mesh LOD.
///
/// The renderer semantics of each variant are fixed here so authored LOD
/// data never carries renderer interpretation of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BillboardPolicy {
    /// Two crossed vertical quads replace the mesh geometry.
    CrossQuads,
    /// One quad that always faces the camera, pivoting about the vertical
    /// axis only.
    YawLocked,
    /// One quad that always faces the camera in all axes.
    Spherical,
}

/// One owned mesh level-of-detail with its geometry payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneMeshLod {
    material_slots: Option<Box<[super::SceneLodMaterialSlot]>>,
    vertex_occlusion_micro: Option<Box<[u32]>>,
    card: Option<super::SceneCard>,
    level_index: u8,
    vertices: Box<[SceneVertex]>,
    indices: Box<[u32]>,
    billboard: Option<BillboardPolicy>,
}

impl SceneMeshLod {
    /// Overrides the placement material with a complete triangle partition.
    pub fn with_material_slots(
        mut self,
        slots: Vec<super::SceneLodMaterialSlot>,
    ) -> Result<Self, SceneValidationError> {
        super::lod_material::validate_slots(&slots, self.indices.len())?;
        self.material_slots = Some(slots.into_boxed_slice());
        Ok(self)
    }

    pub fn material_slots(&self) -> Option<&[super::SceneLodMaterialSlot]> {
        self.material_slots.as_deref()
    }

    /// Supplies one baked occlusion value per vertex, in millionths.
    /// Absence means fully unoccluded. Materials opt into this stream explicitly.
    pub fn with_vertex_occlusion(mut self, values: Vec<u32>) -> Result<Self, SceneValidationError> {
        if values.len() != self.vertices.len() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "vertex occlusion count differs from vertex count",
            });
        }
        for &value in &values {
            super::material_features::bounded("vertex occlusion", value, 1_000_000)?;
        }
        self.vertex_occlusion_micro = Some(values.into_boxed_slice());
        Ok(self)
    }

    /// Optional baked occlusion stream in vertex order.
    pub fn vertex_occlusion_micro(&self) -> Option<&[u32]> {
        self.vertex_occlusion_micro.as_deref()
    }

    pub fn with_card(mut self, card: super::SceneCard) -> Result<Self, SceneValidationError> {
        if self.billboard != Some(card.policy()) {
            return Err(SceneValidationError::UnsupportedInput {
                part: "card policy differs from LOD billboard",
            });
        }
        self.card = Some(card);
        Ok(self)
    }
    pub const fn card(&self) -> Option<&super::SceneCard> {
        self.card.as_ref()
    }

    /// Adopts one level after validating its index buffer closure.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when the vertex or index buffer is
    /// empty, the index count is not a triangle-list multiple, or one index
    /// lies outside the vertex table.
    pub fn new(
        level_index: u8,
        vertices: Vec<SceneVertex>,
        indices: Vec<u32>,
        billboard: Option<BillboardPolicy>,
    ) -> Result<Self, SceneValidationError> {
        let level = Self {
            material_slots: None,
            vertex_occlusion_micro: None,
            card: None,
            level_index,
            vertices: vertices.into_boxed_slice(),
            indices: indices.into_boxed_slice(),
            billboard,
        };
        level.validate()?;
        Ok(level)
    }

    /// Returns the authored level index.
    #[must_use]
    pub const fn level_index(&self) -> u8 {
        self.level_index
    }

    /// Returns the owned vertex payload.
    #[must_use]
    pub fn vertices(&self) -> &[SceneVertex] {
        &self.vertices
    }

    /// Returns the owned triangle-list index buffer.
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Returns the authored billboard policy, when this level carries one.
    #[must_use]
    pub const fn billboard(&self) -> Option<BillboardPolicy> {
        self.billboard
    }

    /// Validates the triangle-list closure of this level.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        const PART: &str = "mesh lod";
        if self.vertices.is_empty() {
            return Err(SceneValidationError::EmptyCollection { part: PART });
        }
        let vertex_count = length_as_u64(&self.vertices, PART)?;
        let index_count = length_as_u64(&self.indices, PART)?;
        if self.indices.is_empty() || index_count % 3 != 0 {
            return Err(SceneValidationError::NonTriangleList {
                part: PART,
                observed: index_count,
            });
        }
        for index in &self.indices {
            if u64::from(*index) >= vertex_count {
                return Err(SceneValidationError::IndexOutOfRange {
                    part: PART,
                    index: u64::from(*index),
                    limit: vertex_count,
                });
            }
        }
        Ok(())
    }
}

/// One owned mesh as a contiguous zero-based chain of LOD levels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneMesh {
    lods: Box<[SceneMeshLod]>,
    max_distances_millimeters: Box<[u64]>,
}

impl SceneMesh {
    /// Adopts the level chain after validating ordering closure.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when the chain is empty or its level
    /// indices are not contiguous from zero. Multi-level chains require
    /// [`Self::with_distances`] instead, because their thresholds cannot be inferred.
    pub fn new(lods: Vec<SceneMeshLod>) -> Result<Self, SceneValidationError> {
        let mesh = Self {
            lods: lods.into_boxed_slice(),
            max_distances_millimeters: Box::default(),
        };
        mesh.validate()?;
        Ok(mesh)
    }

    /// Adopts authored inclusive distance ceilings, one per level. Beyond the
    /// final ceiling the last level remains selected. Distances are measured
    /// from the main camera to the placement origin, adjusted only by an
    /// object's explicit LOD distance scale.
    pub fn with_distances(
        lods: Vec<SceneMeshLod>,
        max_distances_millimeters: Vec<u64>,
    ) -> Result<Self, SceneValidationError> {
        let mesh = Self {
            lods: lods.into_boxed_slice(),
            max_distances_millimeters: max_distances_millimeters.into_boxed_slice(),
        };
        mesh.validate()?;
        Ok(mesh)
    }

    /// Authored inclusive ceilings. A single-level mesh needs no thresholds.
    #[must_use]
    pub fn max_distances_millimeters(&self) -> &[u64] {
        &self.max_distances_millimeters
    }

    /// Returns the owned LOD chain from the nearest level outward.
    #[must_use]
    pub fn lods(&self) -> &[SceneMeshLod] {
        &self.lods
    }

    /// Validates chain presence, contiguity, and every level.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        const PART: &str = "mesh lods";
        if self.lods.is_empty() {
            return Err(SceneValidationError::EmptyCollection { part: PART });
        }
        for (position, lod) in self.lods.iter().enumerate() {
            let expected = contiguous_index(position)?;
            if lod.level_index() != expected {
                return Err(SceneValidationError::LodOrdering {
                    part: PART,
                    position,
                    expected,
                    observed: lod.level_index(),
                });
            }
            lod.validate()?;
        }
        let distances = &self.max_distances_millimeters;
        if !(self.lods.len() == 1 && distances.is_empty())
            && (distances.len() != self.lods.len()
                || distances.first() == Some(&0)
                || distances.windows(2).any(|pair| pair[0] >= pair[1]))
        {
            return Err(SceneValidationError::LodDistances);
        }
        Ok(())
    }
}

/// Computes the required level index of one chain position.
fn contiguous_index(position: usize) -> Result<u8, SceneValidationError> {
    u8::try_from(position)
        .map_err(|_| SceneValidationError::ArithmeticOverflow { part: "mesh lods" })
}

/// Converts one slice length to `u64` with checked conversion.
fn length_as_u64<T>(slice: &[T], part: &'static str) -> Result<u64, SceneValidationError> {
    u64::try_from(slice.len()).map_err(|_| SceneValidationError::ArithmeticOverflow { part })
}

/// Unit rotation quaternion in micro-unit components.
///
/// The four components satisfy `x*x + y*y + z*z + w*w ==
/// 1_000_000 * 1_000_000` to within `ROTATION_NORMALIZATION_TOLERANCE`.
/// Exact equality is not required because independently rounded components
/// need not preserve the squared norm. The tolerance follows the quantization
/// bound above, accepting every correctly rounded unit quaternion. It is not
/// a uniqueness rule: other integer components can also fall inside the bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneRotation {
    /// Raw X component in micro-units.
    pub x: i64,
    /// Raw Y component in micro-units.
    pub y: i64,
    /// Raw Z component in micro-units.
    pub z: i64,
    /// Raw W component in micro-units.
    pub w: i64,
}

impl SceneRotation {
    /// The identity rotation.
    pub const IDENTITY: Self = Self {
        x: 0,
        y: 0,
        z: 0,
        w: ROTATION_MICRO_SCALE,
    };

    /// Validates unit normalization within the quantization tolerance.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when the component squares overflow
    /// their sum or deviate from one squared micro-unit scale by more than
    /// `ROTATION_NORMALIZATION_TOLERANCE`.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        let squares = [self.x, self.y, self.z, self.w]
            .into_iter()
            .try_fold(0_i128, |sum, component| {
                let component = i128::from(component);
                sum.checked_add(component * component)
            })
            .ok_or(SceneValidationError::ArithmeticOverflow {
                part: "scene rotation",
            })?;
        let deviation = (squares - ROTATION_MICRO_SCALE_SQUARED).abs();
        if deviation <= ROTATION_NORMALIZATION_TOLERANCE {
            Ok(())
        } else {
            Err(SceneValidationError::RotationNotNormalized {
                part: "scene rotation",
            })
        }
    }
}

/// Canonical rigid placement: millimeter position, quantized rotation, uniform scale.
///
/// # Coordinate frame
///
/// Scene geometry is expressed in the **native right-handed Z-up frame**:
/// `x` is east, `y` is north, and `z` is elevation. This frame is used
/// uniformly by [`SceneTransform::position_millimeters`], by
/// [`SceneRotation`] axes, and by [`SceneVertex::position_millimeters`], so a
/// transform and the mesh it places always agree.
///
/// The frame is deliberately *not* converted to a renderer's Y-up basis here.
/// Converting per placement would round-trip every coordinate through a basis
/// change for no gain, and mixing converted transforms with unconverted mesh
/// vertices is the classic way to ship a mirrored world. A consumer that needs
/// a Y-up basis must apply the change once, at the view level, as
/// `B(x, y, z) = (x, z, -y)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneTransform {
    /// Canonical position in whole millimeters, native Z-up (east, north, elevation).
    pub position_millimeters: [i64; 3],
    /// Orientation, normalized to within the quantization tolerance.
    pub rotation: SceneRotation,
    /// Strictly positive uniform scale in millionths.
    pub scale_millionths: u64,
}

impl SceneTransform {
    /// Validates rotation normalization and scale positivity.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when the rotation is unnormalized or
    /// the scale is zero.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        self.rotation.validate()?;
        if self.scale_millionths == 0 {
            return Err(SceneValidationError::RangeViolation {
                part: "scene transform scale_millionths",
                minimum: 1,
                maximum: u64::MAX,
                observed: 0,
            });
        }
        Ok(())
    }
}

/// One ordered scene object: canonical placement plus dense resource handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneObject {
    casts_shadow: bool,
    lod_distance_scale_millionths: Option<u32>,
    fade: Option<super::SceneFadeBand>,
    tint: super::SceneTint,
    transform: SceneTransform,
    mesh: MeshHandle,
    material: MaterialHandle,
}

impl SceneObject {
    /// Controls caster participation without affecting visibility or receiving.
    #[must_use]
    pub const fn with_casts_shadow(mut self, casts_shadow: bool) -> Self {
        self.casts_shadow = casts_shadow;
        self
    }

    pub const fn casts_shadow(&self) -> bool {
        self.casts_shadow
    }

    /// Scales LOD ceilings independently of geometry scale, including selection
    /// of card levels and their complementary preceding-level geometry.
    /// Explicit object and material fade endpoints remain in world distance.
    /// Absence preserves authored ceilings. The accepted range is 0.000001..=100.
    pub fn with_lod_distance_scale_millionths(
        mut self,
        scale: u32,
    ) -> Result<Self, SceneValidationError> {
        if !(1..=100_000_000).contains(&scale) {
            return Err(SceneValidationError::RangeViolation {
                part: "object LOD distance scale_millionths",
                minimum: 1,
                maximum: 100_000_000,
                observed: u64::from(scale),
            });
        }
        self.lod_distance_scale_millionths = Some(scale);
        Ok(self)
    }

    pub const fn lod_distance_scale_millionths(&self) -> Option<u32> {
        self.lod_distance_scale_millionths
    }

    /// Replaces placement without discarding its object-specific policies.
    #[must_use]
    pub const fn with_transform(mut self, transform: SceneTransform) -> Self {
        self.transform = transform;
        self
    }

    /// Overrides material fade-in distances for this placement only.
    #[must_use]
    pub const fn with_fade(mut self, band: super::SceneFadeBand) -> Self {
        self.fade = Some(band);
        self
    }
    pub const fn fade(&self) -> Option<super::SceneFadeBand> {
        self.fade
    }

    #[must_use]
    pub const fn with_tint(mut self, tint: super::SceneTint) -> Self {
        self.tint = tint;
        self
    }
    pub const fn tint(&self) -> super::SceneTint {
        self.tint
    }

    /// Adopts one object placement and its resource handles.
    #[must_use]
    pub const fn new(
        transform: SceneTransform,
        mesh: MeshHandle,
        material: MaterialHandle,
    ) -> Self {
        Self {
            casts_shadow: true,
            lod_distance_scale_millionths: None,
            fade: None,
            tint: super::SceneTint::WHITE,
            transform,
            mesh,
            material,
        }
    }

    /// Returns the canonical placement.
    #[must_use]
    pub const fn transform(&self) -> &SceneTransform {
        &self.transform
    }

    /// Returns the rendered mesh handle.
    #[must_use]
    pub const fn mesh(&self) -> MeshHandle {
        self.mesh
    }

    /// Returns the rendered material handle.
    #[must_use]
    pub const fn material(&self) -> MaterialHandle {
        self.material
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex() -> SceneVertex {
        SceneVertex {
            tangent_micro: None,
            position_millimeters: [0, 0, 0],
            normal_micro: [0, 1_000_000, 0],
            texcoord_micro: [0, 0],
        }
    }

    #[test]
    fn triangle_lists_are_validated_inclusively() {
        let lod = SceneMeshLod::new(0, vec![vertex()], vec![0, 0, 0], None)
            .expect("whole triangles validate");
        assert_eq!(lod.level_index(), 0);
        assert!(matches!(
            SceneMeshLod::new(0, vec![vertex()], vec![0, 0], None),
            Err(SceneValidationError::NonTriangleList { observed: 2, .. })
        ));
        assert!(matches!(
            SceneMeshLod::new(0, vec![vertex()], vec![0, 1, 0], None),
            Err(SceneValidationError::IndexOutOfRange {
                index: 1,
                limit: 1,
                ..
            })
        ));
        assert!(matches!(
            SceneMeshLod::new(0, Vec::new(), vec![0, 0, 0], None),
            Err(SceneValidationError::EmptyCollection { .. })
        ));
    }

    #[test]
    fn lod_chains_must_be_contiguous_from_zero() {
        let near = SceneMeshLod::new(0, vec![vertex()], vec![0, 0, 0], None)
            .expect("near level validates");
        let far = SceneMeshLod::new(
            1,
            vec![vertex()],
            vec![0, 0, 0],
            Some(BillboardPolicy::CrossQuads),
        )
        .expect("far level validates");
        assert_eq!(far.billboard(), Some(BillboardPolicy::CrossQuads));
        SceneMesh::with_distances(vec![near, far], vec![10_000, 40_000])
            .expect("contiguous chain validates");
        let drifted = SceneMeshLod::new(2, vec![vertex()], vec![0, 0, 0], None)
            .expect("level payload validates");
        assert!(matches!(
            SceneMesh::new(vec![drifted]),
            Err(SceneValidationError::LodOrdering {
                position: 0,
                expected: 0,
                observed: 2,
                ..
            })
        ));
    }

    #[test]
    fn transforms_require_normalized_rotation_and_positive_scale() {
        assert!(
            SceneTransform {
                position_millimeters: [1, 2, 3],
                rotation: SceneRotation::IDENTITY,
                scale_millionths: 1_000_000,
            }
            .validate()
            .is_ok()
        );
        // A real quantized yaw of 45 degrees about +Z. Its squared norm misses
        // the target by 532_889 micro-units squared, which no exact integer
        // quaternion can avoid, so it must be accepted.
        assert!(
            SceneTransform {
                position_millimeters: [0; 3],
                rotation: SceneRotation {
                    x: 0,
                    y: 0,
                    z: 382_683,
                    w: 923_880,
                },
                scale_millionths: 1_000_000,
            }
            .validate()
            .is_ok()
        );
        assert!(matches!(
            SceneTransform {
                position_millimeters: [0; 3],
                rotation: SceneRotation {
                    x: 0,
                    y: 0,
                    z: 0,
                    w: 999_000,
                },
                scale_millionths: 1_000_000,
            }
            .validate(),
            Err(SceneValidationError::RotationNotNormalized { .. })
        ));
        assert!(matches!(
            SceneTransform {
                position_millimeters: [0; 3],
                rotation: SceneRotation::IDENTITY,
                scale_millionths: 0,
            }
            .validate(),
            Err(SceneValidationError::RangeViolation { observed: 0, .. })
        ));
    }
}
