use super::*;
use crate::CameraPose;

fn placed(position: [i64; 3], rotation: SceneRotation, scale: u64) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: position,
            rotation,
            scale_millionths: scale,
        },
        MeshHandle::from_raw(1),
        MaterialHandle::from_raw(1),
    )
}

#[test]
fn gpu_rotated_and_uniform_scaled_instances_cast_transformed_footprints() {
    let context = context();
    let source = fixture::caster_scene(
        vec![
            placed(
                [0, 0, 1_000],
                SceneRotation {
                    x: 0,
                    y: 0,
                    z: 382_683,
                    w: 923_880,
                },
                2_000_000,
            ),
            placed([1_500, 0, 1_000], SceneRotation::IDENTITY, 500_000),
        ],
        fixture::quad(300),
        None,
        false,
    );
    let baseline = capture_scene(&context, &source, &view()).unwrap();
    let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    // Beyond the unrotated square's x bound, but inside the rotated diamond.
    assert_shadow(&image, &baseline, Vec3::new(-1.45, 0.0, 0.0));
    assert_lit(&image, &baseline, Vec3::new(-1.45, 0.35, 0.0));
    assert_shadow(&image, &baseline, Vec3::new(0.75, 0.0, 0.0));
    assert_lit(&image, &baseline, Vec3::new(1.0, 0.0, 0.0));
}

#[test]
fn gpu_double_sided_backfaces_cast_opaque_and_masked_coverage() {
    let context = context();
    let backface = placed(
        [0, 0, 1_000],
        SceneRotation {
            x: 1_000_000,
            y: 0,
            z: 0,
            w: 0,
        },
        1_000_000,
    );
    for alpha in [None, Some([255, 255, 0, 0])] {
        for double_sided in [false, true] {
            let source =
                fixture::caster_scene(vec![backface], fixture::quad(300), alpha, double_sided);
            let baseline = capture_scene(&context, &source, &view()).unwrap();
            let image = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
            let left = Vec3::new(-0.93, 0.0, 0.0);
            let right = Vec3::new(-0.57, 0.0, 0.0);
            if double_sided {
                assert_shadow(&image, &baseline, left);
                if alpha.is_some() {
                    assert_lit(&image, &baseline, right);
                } else {
                    assert_shadow(&image, &baseline, right);
                }
            } else {
                assert_lit(&image, &baseline, left);
                assert_lit(&image, &baseline, right);
            }
        }
    }
}

pub(super) fn card(policy: BillboardPolicy) -> SceneMesh {
    let horizontal = fixture::quad(300);
    let lod = &horizontal.lods()[0];
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            lod.vertices()
                .iter()
                .map(|v| SceneVertex {
                    tangent_micro: None,
                    position_millimeters: [v.position_millimeters[0], 0, v.position_millimeters[1]],
                    normal_micro: [0, -1_000_000, 0],
                    texcoord_micro: v.texcoord_micro,
                })
                .collect(),
            lod.indices().to_vec(),
            Some(policy),
        )
        .unwrap(),
    ])
    .unwrap()
}

fn camera(eye_millimeters: [i64; 3]) -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters,
            // Offset the target to retain a valid camera roll directly over the pivot.
            target_millimeters: [100, 0, 0],
        },
        *fixture::effects(600_000).camera(),
        view().viewport(),
    )
    .unwrap()
}

fn billboard_receivers(
    context: &GpuContext,
    policy: BillboardPolicy,
    view: &SceneView,
    center_shadowed: bool,
) {
    let source = fixture::caster_scene(
        vec![placed([0, 0, 1_000], SceneRotation::IDENTITY, 1_000_000)],
        card(policy),
        None,
        true,
    );
    let baseline = capture_scene(context, &source, view).unwrap();
    let image = capture_scene_shadowed(context, &source, view, settings()).unwrap();
    for (point, shadowed) in [
        (Vec3::new(-0.75, 0.0, 0.0), center_shadowed),
        (Vec3::new(-0.75, 0.6, 0.0), false),
        (Vec3::new(-1.4, 0.0, 0.0), false),
    ] {
        let delta =
            i16::from(at_view(&baseline, point, view)) - i16::from(at_view(&image, point, view));
        assert!(
            if shadowed {
                delta > 40
            } else {
                delta.abs() <= 2
            },
            "{policy:?}, eye {:?}, receiver {point:?}, shadowed={shadowed}, delta={delta}",
            view.eye_meters()
        );
    }
    assert_eq!(
        image,
        capture_scene_shadowed(context, &source, view, settings()).unwrap()
    );
}

#[test]
fn gpu_billboard_shadows_use_camera_orientation_in_both_passes() {
    let context = context();
    for policy in [BillboardPolicy::YawLocked, BillboardPolicy::Spherical] {
        billboard_receivers(&context, policy, &camera([-4_000, 0, 6_500]), true);
    }
}

#[test]
fn gpu_billboard_overhead_orientation_fallbacks_have_stable_receiver_coverage() {
    let context = context();
    for eye in [[0, 0, 6_500], [2, 0, 6_500], [10, 0, 6_500]] {
        for policy in [BillboardPolicy::YawLocked, BillboardPolicy::Spherical] {
            // At exactly zero horizontal direction yaw holds the authored X-Z
            // card, parallel to the sun rays. Spherical remains a horizontal card.
            let shadowed = policy == BillboardPolicy::Spherical || eye[0] != 0;
            billboard_receivers(&context, policy, &camera(eye), shadowed);
        }
    }
}
