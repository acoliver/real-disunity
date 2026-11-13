//! CPU scene data for linear-blend skinning.

use super::{BillboardPolicy, SceneValidationError};

/// Dense index of one reusable skeleton resource in [`super::SceneGeometry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkeletonHandle(u32);

impl SkeletonHandle {
    /// Adopts one raw skeleton table index.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw skeleton table index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Dense index of one per-instance pose resource in [`super::SceneGeometry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkinPoseHandle(u32);

impl SkinPoseHandle {
    /// Adopts one raw pose table index.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw pose table index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Affine model-space transform stored in fixed-point scene units.
///
/// The linear 3x3 coefficients are signed micro-unit ratios. Translation is
/// in whole millimeters. This avoids floating-point NaN and infinity states
/// in the owned CPU scene contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneAffineMatrix {
    /// Linear transform coefficients in row-major order, millionths.
    pub linear_micro: [[i32; 3]; 3],
    /// Translation in whole millimeters.
    pub translation_millimeters: [i64; 3],
}

impl SceneAffineMatrix {
    /// Affine identity transform.
    pub const IDENTITY: Self = Self {
        linear_micro: [[1_000_000, 0, 0], [0, 1_000_000, 0], [0, 0, 1_000_000]],
        translation_millimeters: [0; 3],
    };
}

/// One vertex's four source-format joint and normalized-byte weight lanes.
///
/// A weight is interpreted as `byte / 255`. Four zero weights explicitly
/// preserve an unweighted source vertex; other sums must be exactly 255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSkinInfluence {
    /// Four joint indices in source u8 representation.
    pub joints: [u8; 4],
    /// Four normalized source weights in u8 representation.
    pub weights: [u8; 4],
}

impl SceneSkinInfluence {
    pub(super) fn validate(
        self,
        vertex: usize,
        joint_count: usize,
    ) -> Result<(), SceneValidationError> {
        for (lane, joint) in self.joints.into_iter().enumerate() {
            if usize::from(joint) >= joint_count {
                return Err(SceneValidationError::SkinJointOutOfRange {
                    vertex,
                    lane,
                    joint,
                    limit: joint_count,
                });
            }
        }
        let sum: u16 = self.weights.into_iter().map(u16::from).sum();
        if sum != 0 && sum != 255 {
            return Err(SceneValidationError::SkinWeightSum {
                vertex,
                observed: sum,
            });
        }
        Ok(())
    }
}

/// Refuses a camera-facing billboard on a skinned LOD.
///
/// Skinning deforms the authored vertices before the model transform, while a camera-facing
/// billboard replaces the authored rotation, so the two cannot be combined. Crossed quads keep
/// the authored rotation and stay admissible.
pub(super) fn refuse_facing_billboard(
    billboard: Option<BillboardPolicy>,
) -> Result<(), SceneValidationError> {
    match billboard {
        Some(BillboardPolicy::YawLocked | BillboardPolicy::Spherical) => {
            Err(SceneValidationError::UnsupportedInput {
                part: "skinned LOD with a camera-facing billboard",
            })
        }
        None | Some(BillboardPolicy::CrossQuads) => Ok(()),
    }
}

/// Reusable joint hierarchy and inverse-bind transforms for skinned LODs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneSkeleton {
    parents: Box<[Option<u32>]>,
    inverse_bind_matrices: Box<[SceneAffineMatrix]>,
}

impl SceneSkeleton {
    /// Adopts a nonempty skeleton and validates parent-before-child ordering.
    pub fn new(
        parents: Vec<Option<u32>>,
        inverse_bind_matrices: Vec<SceneAffineMatrix>,
    ) -> Result<Self, SceneValidationError> {
        let skeleton = Self {
            parents: parents.into_boxed_slice(),
            inverse_bind_matrices: inverse_bind_matrices.into_boxed_slice(),
        };
        skeleton.validate()?;
        Ok(skeleton)
    }

    /// Number of joints in this skeleton.
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.parents.len()
    }

    /// Parent table, with `None` for each root.
    #[must_use]
    pub fn parents(&self) -> &[Option<u32>] {
        &self.parents
    }

    /// Inverse-bind transforms in joint order.
    #[must_use]
    pub fn inverse_bind_matrices(&self) -> &[SceneAffineMatrix] {
        &self.inverse_bind_matrices
    }

    /// Validates a nonempty, count-matched hierarchy and all parent indices.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        if self.parents.is_empty() {
            return Err(SceneValidationError::EmptyCollection {
                part: "skeleton joints",
            });
        }
        if self.parents.len() != self.inverse_bind_matrices.len() {
            return Err(SceneValidationError::SkinResourceCount {
                part: "skeleton inverse bind matrices",
                expected: self.parents.len(),
                observed: self.inverse_bind_matrices.len(),
            });
        }
        for (joint, parent) in self.parents.iter().enumerate() {
            if let Some(parent) = parent
                && usize::try_from(*parent).is_ok_and(|parent| parent >= joint)
            {
                return Err(SceneValidationError::SkeletonParentAfterChild {
                    joint,
                    parent: *parent,
                });
            }
        }
        Ok(())
    }
}

/// Current and previous skinning palettes for one scene instance.
///
/// Each entry is the final mesh-local skinning transform of one joint: the posed joint
/// transform with the skeleton's inverse bind already applied. A deformed vertex is the
/// weighted sum of these transforms applied to its bind position, so the identity
/// [`Self::rest_pose`] draws the bind mesh. The renderer reads palettes directly and never
/// composes the hierarchy or the inverse binds; producers such as an animation evaluator do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneSkinPose {
    current: Box<[SceneAffineMatrix]>,
    previous: Box<[SceneAffineMatrix]>,
}

impl SceneSkinPose {
    /// Adopts current and previous palettes with one matrix per joint.
    pub fn new(
        current: Vec<SceneAffineMatrix>,
        previous: Vec<SceneAffineMatrix>,
    ) -> Result<Self, SceneValidationError> {
        let pose = Self {
            current: current.into_boxed_slice(),
            previous: previous.into_boxed_slice(),
        };
        pose.validate()?;
        Ok(pose)
    }

    /// Constructs a rest pose with identity matrices in both frame palettes.
    pub fn rest_pose(joint_count: usize) -> Result<Self, SceneValidationError> {
        if joint_count == 0 {
            return Err(SceneValidationError::EmptyCollection {
                part: "skin pose joints",
            });
        }
        let identity = vec![SceneAffineMatrix::IDENTITY; joint_count];
        Self::new(identity.clone(), identity)
    }

    /// Number of joints in both palettes.
    #[must_use]
    pub fn joint_count(&self) -> usize {
        self.current.len()
    }

    /// Current frame palette.
    #[must_use]
    pub fn current(&self) -> &[SceneAffineMatrix] {
        &self.current
    }

    /// Previous frame palette.
    #[must_use]
    pub fn previous(&self) -> &[SceneAffineMatrix] {
        &self.previous
    }

    /// Ensures both palettes are nonempty and have equal joint counts.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        if self.current.is_empty() || self.previous.is_empty() {
            return Err(SceneValidationError::EmptyCollection {
                part: "skin pose joints",
            });
        }
        if self.current.len() != self.previous.len() {
            return Err(SceneValidationError::SkinResourceCount {
                part: "previous skin pose palette",
                expected: self.current.len(),
                observed: self.previous.len(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_rejects_parent_after_child() {
        assert!(matches!(
            SceneSkeleton::new(vec![Some(1), None], vec![SceneAffineMatrix::IDENTITY; 2]),
            Err(SceneValidationError::SkeletonParentAfterChild {
                joint: 0,
                parent: 1
            })
        ));
    }

    #[test]
    fn rest_pose_has_identity_current_and_previous_palettes() {
        let pose = SceneSkinPose::rest_pose(2).expect("nonempty rest pose");
        assert_eq!(pose.current(), &[SceneAffineMatrix::IDENTITY; 2]);
        assert_eq!(pose.previous(), &[SceneAffineMatrix::IDENTITY; 2]);
    }

    #[test]
    fn all_zero_influences_remain_explicitly_unweighted() {
        let influence = SceneSkinInfluence {
            joints: [0, 1, 2, 3],
            weights: [0; 4],
        };
        assert_eq!(influence.validate(0, 4), Ok(()));
        assert!(matches!(
            SceneSkinInfluence {
                joints: [0; 4],
                weights: [128, 128, 0, 0]
            }
            .validate(0, 1),
            Err(SceneValidationError::SkinWeightSum {
                vertex: 0,
                observed: 256
            })
        ));
    }
}
