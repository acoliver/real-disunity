//! Skinned meshes through the real object path.
//!
//! The ribbon is three quads on the ground plane, side by side along `x`. Joint 0 holds the left
//! quad, joint 1 the right one, and the middle quad is explicitly unweighted although its joint
//! indices name joint 1. The camera stands on the `x = 0` plane, so everything left of that plane
//! lands in the left half of the frame and everything right of it in the right half.
use super::*;
use crate::test_support::upload_scene;
use worldrunner_render_plan::{
    SceneAffineMatrix, SceneSkeleton, SceneSkinInfluence, SceneSkinPose, SkeletonHandle,
    SkinPoseHandle,
};

use crate::instances::InstanceError;

/// How far the pose carries joint 1 along `+x`.
const SHIFT_MM: i64 = 400;

fn quad_at(low: i64, high: i64) -> Vec<SceneVertex> {
    [
        ([low, -500, 0], [0, 0]),
        ([high, -500, 0], [MICRO, 0]),
        ([high, 500, 0], [MICRO, MICRO]),
        ([low, 500, 0], [0, MICRO]),
    ]
    .map(|(position, texcoord)| vertex(position, texcoord))
    .to_vec()
}

fn influence(joint: u8, weight: u8) -> SceneSkinInfluence {
    SceneSkinInfluence {
        joints: [joint, 0, 0, 0],
        weights: [weight, 0, 0, 0],
    }
}

/// The ribbon: left quad on joint 0, middle quad unweighted, right quad on joint 1.
pub(crate) fn ribbon() -> SceneMesh {
    let mut vertices = quad_at(-1_000, -500);
    vertices.extend(quad_at(-400, -50));
    vertices.extend(quad_at(100, 600));
    let indices = (0..3_u32)
        .flat_map(|quad| [0, 1, 2, 0, 2, 3].map(|index| quad * 4 + index))
        .collect();
    let influences = (0..12)
        .map(|index| match index / 4 {
            0 => influence(0, 255),
            1 => influence(1, 0),
            _ => influence(1, 255),
        })
        .collect();
    let lod = SceneMeshLod::new(0, vertices, indices, None)
        .expect("the ribbon is a valid level")
        .with_skin_influences(SkeletonHandle::from_raw(0), influences)
        .expect("the ribbon influences match its vertices");
    SceneMesh::new(vec![lod]).expect("a one-level chain is valid")
}

pub(crate) fn shifted(millimeters: i64) -> SceneAffineMatrix {
    SceneAffineMatrix {
        translation_millimeters: [millimeters, 0, 0],
        ..SceneAffineMatrix::IDENTITY
    }
}

pub(crate) fn skeleton(joints: usize) -> SceneSkeleton {
    SceneSkeleton::new(
        (0..joints)
            .map(|joint| {
                joint
                    .checked_sub(1)
                    .map(|parent| u32::try_from(parent).unwrap())
            })
            .collect(),
        vec![SceneAffineMatrix::IDENTITY; joints],
    )
    .expect("the fixture skeleton validates")
}

/// A scene with the ribbon posed with joint 1 shifted by `shift` millimetres. Pose 1 has one
/// joint too few for the skeleton.
fn ribbon_scene(shift: i64, pose: Option<SkinPoseHandle>) -> RenderScene {
    let palette = vec![SceneAffineMatrix::IDENTITY, shifted(shift)];
    let poses = vec![
        SceneSkinPose::new(palette.clone(), palette).expect("two joints"),
        SceneSkinPose::rest_pose(1).expect("one joint"),
    ];
    let objects = pose
        .map(|pose| {
            SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )
            .with_skin_pose(pose)
        })
        .into_iter()
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![material(SceneAlphaMode::Opaque, false)],
        )
        .expect("the fixture resources validate"),
        geometry: SceneGeometry::new_with_skinning_resources(
            vec![ribbon()],
            objects,
            vec![skeleton(2)],
            poses,
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: Some(effects()),
    })
    .expect("the fixture scene validates")
}

pub(crate) fn posed(shift: i64) -> RenderScene {
    ribbon_scene(shift, Some(SkinPoseHandle::from_raw(0)))
}

fn column(pixels: &[u8], x: usize) -> Vec<[u8; 4]> {
    (0..TARGET as usize)
        .map(|y| pixels.as_chunks::<4>().0[y * TARGET as usize + x])
        .collect()
}

/// The last column holding any lit pixel.
fn rightmost(pixels: &[u8]) -> usize {
    (0..TARGET as usize)
        .rev()
        .find(|x| column(pixels, *x).iter().any(|t| t[0] > 0 || t[1] > 0))
        .expect("the ribbon is visible")
}

#[test]
fn a_moved_bone_moves_the_pixels_it_weights_and_leaves_the_others_alone() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let rest = render(&context, &posed(0));
    let moved = render(&context, &posed(SHIFT_MM));
    let half = TARGET as usize / 2;
    for x in 0..half {
        assert_eq!(
            column(&rest, x),
            column(&moved, x),
            "column {x} lies left of the camera's plane, where the left quad (joint 0) and the \
             unweighted middle quad were not moved"
        );
    }
    assert_ne!(
        rest, moved,
        "joint 1 carries the right quad, which must move"
    );
    assert!(
        rightmost(&moved) > rightmost(&rest),
        "the right quad shifts towards +x with joint 1"
    );
    assert!(lit_pixels(&rest) > 0 && lit_pixels(&moved) > 0);
}

#[test]
fn the_rest_pose_draws_the_bind_mesh() {
    // Identity palettes leave every vertex where the source put it, so the skinned draw agrees
    // with the same ribbon drawn without influences.
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let skinned = render(&context, &posed(0));
    let source = ribbon();
    let lod = &source.lods()[0];
    let rigid = SceneMesh::new(vec![
        SceneMeshLod::new(0, lod.vertices().to_vec(), lod.indices().to_vec(), None).unwrap(),
    ])
    .unwrap();
    let rigid = build(
        white_texture(),
        &material(SceneAlphaMode::Opaque, false),
        rigid,
        1,
    );
    assert_eq!(skinned, render(&context, &rigid));
}

#[test]
fn a_skinned_object_without_a_pose_or_with_the_wrong_joint_count_is_refused() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let scene = ribbon_scene(0, None);
    let mut gpu = upload_scene(context.device(), context.submission(), &scene, &pipelines)
        .expect("a scene with no placements uploads");
    let object = |pose: Option<SkinPoseHandle>| {
        let placed = SceneObject::new(
            identity(),
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(0),
        );
        pose.map_or(placed, |pose| placed.with_skin_pose(pose))
    };
    assert!(matches!(
        gpu.add_instance(object(None), true),
        Err(InstanceError::MissingSkinPose)
    ));
    assert!(matches!(
        gpu.add_instance(object(Some(SkinPoseHandle::from_raw(1))), true),
        Err(InstanceError::SkinPoseJoints {
            pose_joints: 1,
            skeleton_joints: 2
        })
    ));
    assert!(matches!(
        gpu.add_instance(object(Some(SkinPoseHandle::from_raw(9))), true),
        Err(InstanceError::Resource {
            resource: "skin pose",
            index: 9
        })
    ));
    assert_eq!(gpu.instance_handles().count(), 0);
    gpu.add_instance(object(Some(SkinPoseHandle::from_raw(0))), true)
        .expect("a pose with the skeleton's joints is accepted");
}

#[test]
fn a_pass_that_writes_object_motion_refuses_skinned_geometry_until_it_has_a_previous_pose() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines =
        ScenePipelines::with_taa_velocity(context.device(), TextureFormat::Rgba16Float, 1)
            .expect("the device supports the velocity pipelines");
    let gpu = upload_scene(
        context.device(),
        context.submission(),
        &posed(SHIFT_MM),
        &pipelines,
    )
    .expect("the scene uploads");
    let refusal = gpu
        .prepare_objects(&pipelines, &frame_from(pose()))
        .expect_err("the velocity context has no skinned form");
    assert!(
        refusal.to_string().contains("skinned geometry"),
        "the refusal names what it refuses: {refusal}"
    );
}

/// The ribbon of [`posed`] without influences, as one rigid mesh.
pub(crate) fn rigid() -> RenderScene {
    let source = ribbon();
    let lod = &source.lods()[0];
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, lod.vertices().to_vec(), lod.indices().to_vec(), None).unwrap(),
    ])
    .unwrap();
    build(
        white_texture(),
        &material(SceneAlphaMode::Opaque, false),
        mesh,
        1,
    )
}
