//! A skinned placement is selected by where its posed vertices land, not by its bind geometry.
//!
//! The triangle lies near the origin of its mesh, and the placement stands far to the right of
//! the camera's view. Pose 0 leaves the triangle at its bind position, outside the view. Pose 1
//! carries joint 0 back to the left by the placement's offset, which brings the triangle into
//! view.
use super::compaction::Rig;
use super::skinning::{shifted, skeleton};
use super::*;
use worldrunner_render_plan::{
    SceneAffineMatrix, SceneSkinInfluence, SceneSkinPose, SkeletonHandle, SkinPoseHandle,
};

/// How far right of the camera's view the placement stands, and how far pose 1 carries it back.
const OFFSET_MM: i64 = 30_000;

fn triangle() -> SceneMesh {
    let vertices = vec![
        vertex([-200, -300, 500], [0, 0]),
        vertex([200, -300, 500], [MICRO, 0]),
        vertex([0, 300, 500], [0, MICRO]),
    ];
    let influence = SceneSkinInfluence {
        joints: [0; 4],
        weights: [255, 0, 0, 0],
    };
    let lod = SceneMeshLod::new(0, vertices, vec![0, 1, 2], None)
        .expect("the triangle is a valid level")
        .with_skin_influences(SkeletonHandle::from_raw(0), vec![influence; 3])
        .expect("one influence for each vertex");
    SceneMesh::new(vec![lod]).expect("a one-level chain is valid")
}

fn placed(pose: u32) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [OFFSET_MM, 0, 0],
            ..identity()
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_skin_pose(SkinPoseHandle::from_raw(pose))
}

fn scene() -> RenderScene {
    let back = vec![shifted(-OFFSET_MM)];
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, true)],
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new_with_skinning_resources(
            vec![triangle()],
            vec![placed(0)],
            vec![skeleton(1)],
            vec![
                SceneSkinPose::new(
                    vec![SceneAffineMatrix::IDENTITY],
                    vec![SceneAffineMatrix::IDENTITY],
                )
                .expect("one joint"),
                SceneSkinPose::new(back.clone(), back).expect("one joint"),
            ],
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

#[test]
fn a_pose_that_carries_the_mesh_into_view_selects_and_draws_the_placement() {
    let _session = crate::test_support::SceneTestSession::acquire();
    for ranged_only in [true, false] {
        let mut rig = Rig::over(&scene(), ranged_only);
        let frame = super::compaction::unlit(pose());
        let handle = rig.handles()[0];
        assert_eq!(
            lit_pixels(&rig.render(&frame).pixels),
            0,
            "ranged_only {ranged_only}: the bind pose stands outside the view"
        );
        let caster = |rig: &Rig| rig.gpu.caster_bounds(handle).expect("it casts a shadow");
        let at_bind = caster(&rig).bounds;
        assert!(
            at_bind.min.x > 29.0,
            "the rest pose adds nothing to the bind bounds"
        );
        rig.gpu
            .set_instance_object(handle, placed(1))
            .expect("pose 1 belongs to the scene");
        let posed = caster(&rig).bounds;
        assert!(
            posed.min.x <= 0.0 && posed.max.x >= 0.0,
            "ranged_only {ranged_only}: the caster bounds follow the pose, not the bind geometry"
        );
        assert!(
            lit_pixels(&rig.render(&frame).pixels) > 0,
            "ranged_only {ranged_only}: the posed triangle stands in the view"
        );
        rig.gpu
            .set_instance_object(handle, placed(0))
            .expect("pose 0 belongs to the scene");
        assert_eq!(
            lit_pixels(&rig.render(&frame).pixels),
            0,
            "ranged_only {ranged_only}: switching back leaves the view"
        );
    }
}
