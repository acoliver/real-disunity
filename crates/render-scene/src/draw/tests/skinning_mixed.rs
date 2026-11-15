//! A rigid draw and a skinned draw that follow each other share a pipeline index and material but
//! not a vertex layout, so the second must not reuse the first's prepared pipeline.
use super::compaction::Rig;
use super::skinning::{ribbon, shifted, skeleton};
use super::*;
use worldrunner_render_plan::{SceneAffineMatrix, SceneSkinPose, SkinPoseHandle};

/// The ribbon of [`ribbon`] without influences, as a rigid mesh.
fn rigid_ribbon() -> SceneMesh {
    let source = ribbon();
    let lod = &source.lods()[0];
    SceneMesh::new(vec![
        SceneMeshLod::new(0, lod.vertices().to_vec(), lod.indices().to_vec(), None).unwrap(),
    ])
    .unwrap()
}

fn placed(mesh: u32, y: i64) -> SceneObject {
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0, y, 0],
            ..identity()
        },
        MeshHandle::from_raw(mesh),
        MaterialHandle::from_raw(0),
    );
    if mesh == 1 {
        object.with_skin_pose(SkinPoseHandle::from_raw(0))
    } else {
        object
    }
}

/// The rigid ribbon (mesh 0) and the skinned ribbon (mesh 1) on one material, two placements of
/// each, with only the placements named in `keep` present.
fn scene(keep: &[usize]) -> RenderScene {
    let objects = [
        placed(0, -1_200),
        placed(0, -1_300),
        placed(1, 1_200),
        placed(1, 1_300),
    ];
    let palette = vec![SceneAffineMatrix::IDENTITY, shifted(300)];
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new_with_skinning_resources(
            vec![rigid_ribbon(), ribbon()],
            keep.iter().map(|&index| objects[index]).collect(),
            vec![skeleton(2)],
            vec![SceneSkinPose::new(palette.clone(), palette).expect("two joints")],
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

fn lit(scene: &RenderScene, ranged_only: bool) -> usize {
    lit_pixels(&Rig::over(scene, ranged_only).render(&frame()).pixels)
}

#[test]
fn consecutive_rigid_and_skinned_draws_each_use_their_own_layout_on_both_routes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    for ranged_only in [true, false] {
        let both = lit(&scene(&[0, 1, 2, 3]), ranged_only);
        let rigid = lit(&scene(&[0, 1]), ranged_only);
        let skinned = lit(&scene(&[2, 3]), ranged_only);
        assert!(rigid > 0 && skinned > 0, "ranged_only {ranged_only}");
        assert_eq!(
            both,
            rigid + skinned,
            "ranged_only {ranged_only}: the two meshes occupy separate parts of the frame"
        );
    }
}

#[test]
fn the_compacted_route_is_taken_for_mixed_layouts() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::over(&scene(&[0, 1, 2, 3]), false);
    let pixels = rig.render(&frame()).pixels;
    assert!(rig.compacted_draws() >= 1);
    assert_eq!(
        pixels,
        Rig::over(&scene(&[0, 1, 2, 3]), true)
            .render(&frame())
            .pixels
    );
}
