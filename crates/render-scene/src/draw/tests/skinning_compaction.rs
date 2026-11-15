//! A compacted view gathers the skin descriptors with the instance records.
//!
//! Three placements of one ribbon share a batch and differ only in pose. A pose switch leaves the
//! 80-byte placement record unchanged, so these frames only come out right when the descriptor
//! stream is uploaded and gathered on its own.
use super::compaction::Rig;
use super::skinning::{ribbon, shifted, skeleton};
use super::*;
use worldrunner_render_plan::{SceneAffineMatrix, SceneSkinPose, SkinPoseHandle};

fn pose_shifting(millimeters: i64) -> SceneSkinPose {
    let palette = vec![SceneAffineMatrix::IDENTITY, shifted(millimeters)];
    SceneSkinPose::new(palette.clone(), palette).expect("two joints")
}

fn placed(y: i64, pose: u32) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [0, y, 0],
            ..identity()
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_skin_pose(SkinPoseHandle::from_raw(pose))
}

fn scene() -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new_with_skinning_resources(
            vec![ribbon()],
            vec![placed(1_200, 0), placed(0, 1), placed(-1_200, 2)],
            vec![skeleton(2)],
            vec![pose_shifting(0), pose_shifting(500), pose_shifting(-300)],
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

fn frame() -> FrameUniform {
    super::compaction::unlit(pose())
}

#[test]
fn compacted_and_ranged_drawing_agree_on_skinned_instances_and_pose_switches() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut compacted = Rig::over(&scene(), false);
    let mut ranged = Rig::over(&scene(), true);
    let first = compacted.render(&frame());
    assert_eq!(first.pixels, ranged.render(&frame()).pixels);
    assert!(compacted.compacted_draws() >= 1);
    // The middle placement takes the pose of the last one; only its descriptor changes.
    let mut previous = first.pixels;
    for pose in [2, 0] {
        for rig in [&mut compacted, &mut ranged] {
            let handle = rig.handles()[1];
            rig.gpu
                .set_instance_object(handle, placed(0, pose))
                .expect("the pose belongs to the scene");
        }
        let (a, b) = (compacted.render(&frame()), ranged.render(&frame()));
        assert_eq!(a.pixels, b.pixels);
        assert_ne!(
            a.pixels, previous,
            "pose {pose} must move the middle placement"
        );
        previous = a.pixels;
    }
}
