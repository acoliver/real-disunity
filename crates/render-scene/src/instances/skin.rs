//! What a placement's skin pose is checked against, and the descriptor it is drawn with.

use worldrunner_render_plan::{RenderScene, SceneMesh, SceneObject, SceneSkinPose, SkeletonHandle};

use super::InstanceError;
use crate::skinning::{PoseRange, ScenePalette, palette_rows};

/// The skeleton joint counts, the skeletons of each mesh's skinned levels and the range of each
/// pose in the scene palette.
///
/// Skeletons and poses are fixed for the life of a resident scene: appends add meshes and
/// materials, and a mesh replacement may change which skeleton a level uses but not the tables.
#[derive(Debug, Default)]
pub(crate) struct SkinTables {
    skeleton_joints: Vec<u32>,
    /// For each mesh, the skeleton of each skinned level. Empty for a rigid mesh.
    mesh_skeletons: Vec<Vec<SkeletonHandle>>,
    poses: Vec<PoseRange>,
    /// How far each pose can carry a point, indexed like `poses`.
    reaches: Vec<PoseReach>,
}

/// How far the joints of one pose can move a point of the mesh they skin, over the current and
/// the previous palette.
///
/// A joint carries the point `v` to `A v + t`, so it moves it by at most `|(A - I) v| + |t|`,
/// and `|(A - I) v|` is at most the Frobenius norm of `A - I` times `|v|`. A skinned vertex
/// blends its joints with weights that sum to one, so it moves by no more than the joint that
/// moves it furthest.
#[derive(Clone, Copy, Debug, Default)]
struct PoseReach {
    /// The largest Frobenius norm of a joint's linear part minus the identity.
    linear: f32,
    /// The largest length of a joint's translation, in metres.
    translation: f32,
}

impl PoseReach {
    fn of(pose: &SceneSkinPose) -> Self {
        let joints = pose.current().iter().chain(pose.previous());
        joints.fold(Self::default(), |reach, joint| {
            let rows = palette_rows(joint);
            let mut linear = 0.0_f32;
            let mut translation = 0.0_f32;
            for (row, values) in rows.iter().enumerate() {
                for (column, value) in values[..3].iter().enumerate() {
                    let rest = if row == column { 1.0 } else { 0.0 };
                    linear += (value - rest) * (value - rest);
                }
                translation += values[3] * values[3];
            }
            Self {
                linear: reach.linear.max(linear.sqrt()),
                translation: reach.translation.max(translation.sqrt()),
            }
        })
    }
}

/// The skeletons of the skinned levels of `mesh`, in level order.
pub(crate) fn mesh_skeletons(mesh: &SceneMesh) -> Vec<SkeletonHandle> {
    mesh.lods()
        .iter()
        .filter_map(worldrunner_render_plan::SceneMeshLod::skeleton)
        .collect()
}

impl SkinTables {
    /// The tables of `scene`. A scene whose palettes overflow the 32-bit entry index has no
    /// pose ranges here; preparation refuses such a scene before any placement is made.
    pub(crate) fn new(scene: &RenderScene) -> Self {
        let geometry = scene.geometry();
        Self {
            skeleton_joints: geometry
                .skeletons()
                .iter()
                .map(|skeleton| u32::try_from(skeleton.joint_count()).unwrap_or(u32::MAX))
                .collect(),
            mesh_skeletons: geometry.meshes().iter().map(mesh_skeletons).collect(),
            poses: ScenePalette::ranges_of(geometry.skin_poses()).unwrap_or_default(),
            reaches: geometry.skin_poses().iter().map(PoseReach::of).collect(),
        }
    }

    /// How far, in metres of the mesh's own space, the pose of `object` can carry a point that
    /// lies within `radius` metres of the mesh origin. Zero for a rigid mesh.
    pub(crate) fn reach(&self, object: &SceneObject, radius: f32) -> f32 {
        let mesh = usize::try_from(object.mesh().raw()).expect("u32 table index fits usize");
        let Some(reach) = object
            .skin_pose()
            .filter(|_| self.is_skinned(mesh))
            .and_then(|pose| self.reaches.get(pose.raw() as usize))
        else {
            return 0.0;
        };
        // Rounding in the palette and in the vertex stage stays within the margin.
        (reach.linear * radius + reach.translation) * 1.000_01
    }

    /// Whether `mesh` has a skinned level, so its placements carry a skin descriptor.
    pub(crate) fn is_skinned(&self, mesh: usize) -> bool {
        self.mesh_skeletons
            .get(mesh)
            .is_some_and(|skeletons| !skeletons.is_empty())
    }

    /// Makes `meshes` addressable after the ones the scene started with.
    pub(crate) fn extend_meshes(&mut self, meshes: Vec<Vec<SkeletonHandle>>) {
        self.mesh_skeletons.extend(meshes);
    }

    /// Takes the skeletons of the replaced mesh `index`.
    pub(crate) fn replace_mesh(&mut self, index: usize, skeletons: Vec<SkeletonHandle>) {
        self.mesh_skeletons[index] = skeletons;
    }

    /// The descriptor `object` is drawn with, or `None` for a rigid mesh.
    ///
    /// A skinned object has no motion history here, so its previous palette is its current one.
    ///
    /// # Errors
    ///
    /// Fails when a skinned mesh's object has no pose, a pose outside the scene's table, or a
    /// pose with other joint counts than the skeletons of the mesh's skinned levels.
    pub(crate) fn check(&self, object: &SceneObject) -> Result<Option<[u32; 4]>, InstanceError> {
        let mesh = usize::try_from(object.mesh().raw()).expect("u32 table index fits usize");
        let Some(skeletons) = self.mesh_skeletons.get(mesh).filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let handle = object.skin_pose().ok_or(InstanceError::MissingSkinPose)?;
        let range = self
            .poses
            .get(handle.raw() as usize)
            .ok_or(InstanceError::Resource {
                resource: "skin pose",
                index: handle.raw(),
            })?;
        for skeleton in skeletons {
            let joints = self.skeleton_joints[skeleton.raw() as usize];
            if joints != range.joints {
                return Err(InstanceError::SkinPoseJoints {
                    pose_joints: range.joints,
                    skeleton_joints: joints,
                });
            }
        }
        Ok(Some(range.descriptor(false)))
    }
}
