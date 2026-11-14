use super::{
    SceneMesh, SceneObject, SceneSkeleton, SceneSkinPose, SceneValidationError, SkeletonHandle,
};

/// Mesh, skeleton and pose tables plus ordered objects of one scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneGeometry {
    pub(super) meshes: std::sync::Arc<Vec<SceneMesh>>,
    pub(super) objects: Box<[SceneObject]>,
    pub(super) skeletons: Box<[SceneSkeleton]>,
    pub(super) skin_poses: Box<[SceneSkinPose]>,
}

impl SceneGeometry {
    /// Adopts the mesh table and ordered objects.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when mesh chains, transforms or
    /// skinning cross-references violate their closure.
    pub fn new(
        meshes: Vec<SceneMesh>,
        objects: Vec<SceneObject>,
    ) -> Result<Self, SceneValidationError> {
        Self::new_with_skinning_resources(meshes, objects, Vec::new(), Vec::new())
    }

    /// Adopts meshes, objects, reusable skeletons and per-instance poses together.
    pub fn new_with_skinning_resources(
        meshes: Vec<SceneMesh>,
        objects: Vec<SceneObject>,
        skeletons: Vec<SceneSkeleton>,
        skin_poses: Vec<SceneSkinPose>,
    ) -> Result<Self, SceneValidationError> {
        let geometry = Self {
            meshes: meshes.into(),
            objects: objects.into_boxed_slice(),
            skeletons: skeletons.into_boxed_slice(),
            skin_poses: skin_poses.into_boxed_slice(),
        };
        geometry.validate()?;
        Ok(geometry)
    }

    /// Returns the owned mesh table.
    #[must_use]
    pub fn meshes(&self) -> &[SceneMesh] {
        &self.meshes
    }

    /// Returns reusable skeleton resources.
    #[must_use]
    pub fn skeletons(&self) -> &[SceneSkeleton] {
        &self.skeletons
    }

    /// Returns per-instance skinning poses.
    #[must_use]
    pub fn skin_poses(&self) -> &[SceneSkinPose] {
        &self.skin_poses
    }

    /// Returns the ordered objects in authored draw order.
    #[must_use]
    pub fn objects(&self) -> &[SceneObject] {
        &self.objects
    }

    /// Validates geometry, skin resource tables, and their cross-references.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        for mesh in self.meshes.iter() {
            mesh.validate()?;
        }
        for skeleton in &self.skeletons {
            skeleton.validate()?;
        }
        for pose in &self.skin_poses {
            pose.validate()?;
        }
        for mesh in self.meshes.iter() {
            self.validate_mesh_skinning(mesh)?;
        }
        self.validate_objects()
    }

    /// Checks one mesh's skinned LODs against this geometry's skeleton table.
    pub(in crate::scene) fn validate_mesh_skinning(
        &self,
        mesh: &SceneMesh,
    ) -> Result<(), SceneValidationError> {
        let skeleton_limit = table_len(self.skeletons.len(), "skeleton table")?;
        for lod in mesh.lods() {
            let Some(skeleton_handle) = lod.skeleton() else {
                continue;
            };
            require_skin_handle(
                "mesh lod",
                "skeleton",
                skeleton_handle.raw(),
                skeleton_limit,
            )?;
            let skeleton = &self.skeletons[skeleton_handle.raw() as usize];
            let Some(influences) = lod.skin_influences() else {
                return Err(SceneValidationError::UnsupportedInput {
                    part: "skin influences and skeleton must be supplied together",
                });
            };
            for (vertex, influence) in influences.iter().copied().enumerate() {
                influence.validate(vertex, skeleton.joint_count())?;
            }
        }
        Ok(())
    }

    /// Checks every object's mesh and skin pose handles and pose joint counts.
    pub(in crate::scene) fn validate_objects(&self) -> Result<(), SceneValidationError> {
        let mesh_limit = table_len(self.meshes.len(), "mesh table")?;
        let pose_limit = table_len(self.skin_poses.len(), "skin pose table")?;
        for object in &self.objects {
            object.transform().validate()?;
            require_skin_handle("scene object", "mesh", object.mesh().raw(), mesh_limit)?;
            let mesh = &self.meshes[object.mesh().raw() as usize];
            let has_skin = mesh.lods().iter().any(|lod| lod.skeleton().is_some());
            let pose = object.skin_pose();
            if has_skin && pose.is_none() {
                return Err(SceneValidationError::MissingSkinPose);
            }
            if let Some(pose_handle) = pose {
                require_skin_handle("scene object", "skin pose", pose_handle.raw(), pose_limit)?;
                let pose = &self.skin_poses[pose_handle.raw() as usize];
                for lod in mesh.lods() {
                    if let Some(skeleton_handle) = lod.skeleton() {
                        self.validate_pose_skeleton(pose, skeleton_handle)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_pose_skeleton(
        &self,
        pose: &SceneSkinPose,
        skeleton_handle: SkeletonHandle,
    ) -> Result<(), SceneValidationError> {
        let skeleton = &self.skeletons[skeleton_handle.raw() as usize];
        if pose.joint_count() != skeleton.joint_count() {
            return Err(SceneValidationError::SkinPoseJointCountMismatch {
                pose_joints: pose.joint_count(),
                skeleton_joints: skeleton.joint_count(),
            });
        }
        Ok(())
    }
}

fn table_len(len: usize, part: &'static str) -> Result<u64, SceneValidationError> {
    u64::try_from(len).map_err(|_| SceneValidationError::ArithmeticOverflow { part })
}

fn require_skin_handle(
    part: &'static str,
    resource: &'static str,
    handle: u32,
    limit: u64,
) -> Result<(), SceneValidationError> {
    if u64::from(handle) < limit {
        Ok(())
    } else {
        Err(SceneValidationError::HandleOutOfRange {
            part,
            resource,
            handle,
            limit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{
        BillboardPolicy, MaterialHandle, MeshHandle, SceneAffineMatrix, SceneMeshLod,
        SceneRotation, SceneSkinInfluence, SceneTransform, SkinPoseHandle,
    };

    fn vertex() -> super::super::SceneVertex {
        super::super::SceneVertex {
            tangent_micro: None,
            position_millimeters: [0; 3],
            normal_micro: [0, 1_000_000, 0],
            texcoord_micro: [0; 2],
        }
    }

    fn transform() -> SceneTransform {
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        }
    }

    fn skeleton(count: usize) -> SceneSkeleton {
        SceneSkeleton::new(vec![None; count], vec![SceneAffineMatrix::IDENTITY; count])
            .expect("synthetic skeleton validates")
    }

    fn skin_lod(influence: SceneSkinInfluence) -> SceneMeshLod {
        SceneMeshLod::new(0, vec![vertex()], vec![0, 0, 0], None)
            .expect("synthetic triangle validates")
            .with_skin_influences(SkeletonHandle::from_raw(0), vec![influence])
            .expect("one influence per vertex")
    }

    fn geometry(
        lod: SceneMeshLod,
        pose: Option<SkinPoseHandle>,
        skeleton_count: usize,
        pose_count: usize,
    ) -> Result<SceneGeometry, SceneValidationError> {
        let mut object = SceneObject::new(
            transform(),
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(0),
        );
        if let Some(pose) = pose {
            object = object.with_skin_pose(pose);
        }
        SceneGeometry::new_with_skinning_resources(
            vec![SceneMesh::new(vec![lod]).expect("mesh validates")],
            vec![object],
            vec![skeleton(skeleton_count)],
            vec![SceneSkinPose::rest_pose(pose_count).expect("pose")],
        )
    }

    fn influence(joints: [u8; 4], weights: [u8; 4]) -> SceneSkinInfluence {
        SceneSkinInfluence { joints, weights }
    }

    #[test]
    fn camera_facing_billboards_cannot_be_skinned() {
        for policy in [BillboardPolicy::YawLocked, BillboardPolicy::Spherical] {
            let lod = SceneMeshLod::new(0, vec![vertex()], vec![0, 0, 0], Some(policy))
                .expect("synthetic billboard triangle validates");
            assert!(matches!(
                lod.with_skin_influences(
                    SkeletonHandle::from_raw(0),
                    vec![influence([0; 4], [255, 0, 0, 0])]
                ),
                Err(SceneValidationError::UnsupportedInput { .. })
            ));
        }
    }

    #[test]
    fn crossed_quads_and_plain_lods_can_be_skinned() {
        for policy in [None, Some(BillboardPolicy::CrossQuads)] {
            let lod = SceneMeshLod::new(0, vec![vertex()], vec![0, 0, 0], policy)
                .expect("synthetic triangle validates")
                .with_skin_influences(
                    SkeletonHandle::from_raw(0),
                    vec![influence([0; 4], [255, 0, 0, 0])],
                )
                .expect("fixed-rotation LOD accepts influences");
            assert!(lod.validate().is_ok());
        }
    }

    #[test]
    fn wrong_influence_count_is_rejected() {
        let lod = SceneMeshLod::new(0, vec![vertex()], vec![0, 0, 0], None)
            .expect("synthetic triangle validates");
        assert!(matches!(
            lod.with_skin_influences(SkeletonHandle::from_raw(0), Vec::new()),
            Err(SceneValidationError::SkinInfluenceCount {
                expected: 1,
                observed: 0
            })
        ));
    }

    #[test]
    fn joint_indices_are_checked_even_in_zero_weight_lanes() {
        let result = geometry(
            skin_lod(influence([0, 0, 0, 2], [255, 0, 0, 0])),
            Some(SkinPoseHandle::from_raw(0)),
            2,
            2,
        );
        assert!(matches!(
            result,
            Err(SceneValidationError::SkinJointOutOfRange {
                vertex: 0,
                lane: 3,
                joint: 2,
                limit: 2
            })
        ));
    }

    #[test]
    fn skinned_object_requires_a_pose() {
        let result = geometry(skin_lod(influence([0; 4], [255, 0, 0, 0])), None, 1, 1);
        assert!(matches!(result, Err(SceneValidationError::MissingSkinPose)));
    }

    #[test]
    fn pose_joint_count_must_match_lod_skeleton() {
        let result = geometry(
            skin_lod(influence([0; 4], [255, 0, 0, 0])),
            Some(SkinPoseHandle::from_raw(0)),
            2,
            1,
        );
        assert!(matches!(
            result,
            Err(SceneValidationError::SkinPoseJointCountMismatch {
                pose_joints: 1,
                skeleton_joints: 2
            })
        ));
    }

    #[test]
    fn all_zero_weights_are_preserved_as_explicit_unweighted_vertices() {
        let all_zero = influence([0; 4], [0; 4]);
        let geometry = geometry(skin_lod(all_zero), Some(SkinPoseHandle::from_raw(0)), 1, 1)
            .expect("zero-weight source vertex is valid");
        assert_eq!(
            geometry.meshes()[0].lods()[0].skin_influences(),
            Some(&[all_zero][..])
        );
    }
}
