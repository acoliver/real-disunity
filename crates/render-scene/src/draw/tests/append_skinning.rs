//! Skinned meshes appended to or swapped into a resident scene must fit its skeleton and pose
//! tables, and a refusal leaves the resident scene as it was.
use super::skinning::{ribbon, shifted, skeleton};
use super::*;
use crate::draw::{DrawError, PreparedAppend, PreparedMeshReplacement};
use crate::instances::InstanceError;
use crate::instances::skin::SkinTables;
use crate::test_support::upload_scene;
use worldrunner_render_plan::{
    SceneAppend, SceneSkinInfluence, SceneSkinPose, SkeletonHandle, SkinPoseHandle,
};

/// The ribbon's vertices skinned to `skeleton`, the last quad to `last_joint`.
fn ribbon_on(skeleton: u32, last_joint: u8) -> SceneMesh {
    let source = ribbon();
    let lod = &source.lods()[0];
    let influences = (0..12)
        .map(|vertex| SceneSkinInfluence {
            joints: [if vertex < 8 { 0 } else { last_joint }, 0, 0, 0],
            weights: [255, 0, 0, 0],
        })
        .collect();
    let lod = SceneMeshLod::new(0, lod.vertices().to_vec(), lod.indices().to_vec(), None)
        .expect("the ribbon is a valid level")
        .with_skin_influences(SkeletonHandle::from_raw(skeleton), influences)
        .expect("the influences match the vertices");
    SceneMesh::new(vec![lod]).expect("a one-level chain is valid")
}

fn palette(joints: usize) -> SceneSkinPose {
    let palette = vec![shifted(0); joints];
    SceneSkinPose::new(palette.clone(), palette).expect("a pose with joints")
}

/// A scene of one two-joint skinned mesh, a two-joint skeleton and a three-joint skeleton, with
/// a pose for each, and no placements.
fn scene() -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new_with_skinning_resources(
            vec![ribbon_on(0, 1)],
            Vec::new(),
            vec![skeleton(2), skeleton(3)],
            vec![palette(2), palette(3)],
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

fn placed(mesh: u32, pose: u32) -> SceneObject {
    SceneObject::new(
        identity(),
        MeshHandle::from_raw(mesh),
        MaterialHandle::from_raw(0),
    )
    .with_skin_pose(SkinPoseHandle::from_raw(pose))
}

#[test]
fn a_skeleton_outside_the_table_is_an_error_where_a_placement_is_admitted() {
    let mut tables = SkinTables::new(&scene());
    tables.extend_meshes(vec![vec![SkeletonHandle::from_raw(9)]]);
    assert!(matches!(
        tables.check(&placed(1, 0)),
        Err(InstanceError::Resource {
            resource: "skeleton",
            index: 9
        })
    ));
}

#[test]
fn an_append_naming_skins_the_scene_lacks_is_refused_and_changes_nothing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu = upload_scene(context.device(), context.submission(), &scene(), &pipelines)
        .expect("the scene uploads");
    let base = gpu.resource_base();
    // Skeleton 9 is not resident, and joint 3 is beyond the two joints of skeleton 0.
    for (skeleton, last_joint) in [(9, 1), (0, 3)] {
        let append = SceneAppend {
            meshes: vec![ribbon_on(skeleton, last_joint)],
            ..SceneAppend::default()
        };
        let prepared = PreparedAppend::new(base, &append).expect("the mesh packs on its own");
        let refusal = gpu
            .append_prepared(
                context.device(),
                context.submission(),
                &pipelines,
                prepared,
                &append,
            )
            .expect_err("the resident tables cannot skin this mesh");
        assert!(
            matches!(refusal, DrawError::SkinReference { mesh: 1, .. }),
            "{refusal}"
        );
        assert_eq!(gpu.resource_base(), base);
        assert!(matches!(
            gpu.add_instance(placed(1, 0), true),
            Err(InstanceError::Resource {
                resource: "mesh",
                index: 1
            })
        ));
    }
}

#[test]
fn a_replacement_naming_skins_the_scene_lacks_is_refused_before_a_placement_can_use_it() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let source = scene();
    let mut gpu =
        upload_scene(context.device(), context.submission(), &source, &pipelines).unwrap();
    let generation = gpu.caster_generation();
    for (skeleton, last_joint) in [(9, 1), (0, 3)] {
        let replacement = PreparedMeshReplacement::new(
            &source,
            MeshHandle::from_raw(0),
            &ribbon_on(skeleton, last_joint),
        )
        .unwrap();
        let refusal = gpu
            .replace_mesh_prepared(context.device(), replacement)
            .expect_err("the resident tables cannot skin this mesh");
        assert!(
            matches!(refusal, DrawError::SkinReference { mesh: 0, .. }),
            "{refusal}"
        );
        assert_eq!(gpu.caster_generation(), generation);
        gpu.add_instance(placed(0, 0), true)
            .expect("the mesh still has the skin it was uploaded with");
    }
}

#[test]
fn a_replacement_that_a_live_pose_cannot_drive_is_refused_and_changes_nothing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let source = scene();
    let mut gpu =
        upload_scene(context.device(), context.submission(), &source, &pipelines).unwrap();
    let handle = gpu
        .add_instance(placed(0, 0), true)
        .expect("the two-joint pose drives the two-joint mesh");
    let (base, generation) = (gpu.resource_base(), gpu.caster_generation());
    // The replacement skins to the three-joint skeleton and weights joint 2.
    let replacement = || {
        PreparedMeshReplacement::new(&source, MeshHandle::from_raw(0), &ribbon_on(1, 2)).unwrap()
    };
    let refusal = gpu
        .replace_mesh_prepared(context.device(), replacement())
        .expect_err("the live placement's pose has two joints");
    assert!(
        matches!(refusal, DrawError::SkinReference { mesh: 0, .. }),
        "{refusal}"
    );
    assert_eq!(gpu.resource_base(), base);
    assert_eq!(gpu.caster_generation(), generation);
    assert_eq!(gpu.instance_handles().count(), 1);
    // Without the placement nothing contradicts the replacement, and a three-joint pose takes it.
    gpu.remove_instance(handle).unwrap();
    gpu.replace_mesh_prepared(context.device(), replacement())
        .expect("no live placement holds the two-joint pose");
    gpu.add_instance(placed(0, 1), true)
        .expect("the three-joint pose drives the replacement");
    assert!(matches!(
        gpu.add_instance(placed(0, 0), true),
        Err(InstanceError::SkinPoseJoints {
            pose_joints: 2,
            skeleton_joints: 3
        })
    ));
}
