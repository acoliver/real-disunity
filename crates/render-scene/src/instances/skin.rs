//! What a placement's skin pose is checked against, and the descriptor it is drawn with.

use worldrunner_render_plan::{RenderScene, SceneMesh, SceneObject, SkeletonHandle};

use super::InstanceError;
use crate::skinning::{PoseRange, ScenePalette};

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
        }
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
