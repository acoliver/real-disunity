use super::{decal, maps::mapped_scene};
use crate::draw::tests::{
    COLOR_FORMAT, context, foliage, frame_from,
    normal_frames::{assert_normal, diagnostic},
    pose,
};
use crate::draw::{SceneGpu, ScenePipelines};
use glam::{Quat, Vec3};
use worldrunner_render_plan::{SceneDecalNormalFrame, SceneRotation};

#[test]
fn correction_frame_reaches_forward_and_metadata_on_rotated_planes_and_slopes() {
    let context = context();
    let layouts = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for slope in [false, true] {
        for rotation in [
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Quat::from_rotation_z(0.7) * Quat::from_rotation_y(0.4),
        ] {
            for frame in [
                SceneDecalNormalFrame::Projection,
                SceneDecalNormalFrame::ReceiverWorld,
            ] {
                let mut d = decal();
                let q = (glam::Vec4::from_array(rotation.to_array()) * 1_000_000.0)
                    .round()
                    .as_ivec4()
                    .to_array()
                    .map(i64::from);
                d.transform.rotation = SceneRotation {
                    x: q[0],
                    y: q[1],
                    z: q[2],
                    w: q[3],
                };
                d.dimensions_millimeters = [4000; 3];
                d.edge_fade_micro = 0;
                d.angle_cutoff_micro = 0;
                d.angle_fade_micro = 0;
                d.mapping.normal_frame = frame;
                let scene = mapped_scene(slope).with_decals(vec![d]).unwrap();
                let mut gpu =
                    SceneGpu::upload(context.device(), context.submission(), &scene, &layouts)
                        .unwrap();
                gpu.set_frame(context.queue(), frame_from(pose()));
                gpu.frame_group = gpu.decals.group();
                let expected = oracle(slope, rotation, frame);
                for source in foliage::geometry_sources().iter().take(3) {
                    let pixels = foliage::pass_color(
                        &context,
                        &gpu,
                        &layouts,
                        &diagnostic(source),
                        "fs_opaque",
                    );
                    assert_normal(&pixels, expected);
                }
            }
        }
    }
}

fn oracle(slope: bool, rotation: Quat, frame: SceneDecalNormalFrame) -> Vec3 {
    let geometric = if slope {
        Vec3::new(-0.5, 0.0, 1.0).normalize()
    } else {
        Vec3::Z
    };
    let x = 1.0_f32 / 255.0;
    let y = 127.0_f32 / 255.0;
    let detail = Vec3::new(y, -x, (1.0 - x * x - y * y).sqrt());
    match frame {
        SceneDecalNormalFrame::ReceiverWorld => {
            Quat::from_rotation_arc(Vec3::Z, geometric) * detail
        }
        SceneDecalNormalFrame::Projection => {
            rotation * (Quat::from_rotation_arc(Vec3::Z, rotation.conjugate() * geometric) * detail)
        }
    }
}
