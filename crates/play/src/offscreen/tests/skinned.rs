//! A skinned renderer reaches the frame like any other renderer: the world lists it at its root
//! bone's matrix times the bind pose of that bone and the adapter draws it, with its mesh's blend
//! channels available to a pass that reads them.

use glam::{Quat, Vec3};
use worldrunner_render_program::ids::{MeshId, RendererId};
use worldrunner_render_program::unity::offscreen::frame::ChannelFormat;
use worldrunner_world_simulation::graphics::OffscreenWork;
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, MeshRenderer, ParentSpace, RendererKind, SkinBinding, SkinRoot,
};

use super::super::assets::mesh_of;
use super::super::frame::{RendererSkip, adapt};
use super::assets::{channel, floats, mesh, sources};
use super::frame::settings;
use super::support::{
    assert_close, available, camera, material, matrix, object, render_texture, shader, snapshot,
    world,
};

const MESH: u64 = (1 << 41) + 7;

#[test]
fn a_skinned_renderer_is_drawn_where_its_root_bone_puts_it_and_no_renderer_is_skipped() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let shader = shader(&mut world, "Draw");
    let material = material(&mut world, shader);
    let rig = object(&mut world, 0, Vec3::new(5.0, 0.0, 0.0), Quat::IDENTITY);
    world
        .add_component(rig, ComponentDesc::Animator { enabled: true })
        .unwrap();
    let body = object(
        &mut world,
        0,
        Vec3::new(0.0, 3.0, 0.0),
        Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2),
    );
    world
        .set_parent(body, Some(rig), ParentSpace::KeepLocal)
        .unwrap();
    let mut skinned = MeshRenderer::of_kind(RendererKind::Skinned);
    skinned.material_count = 1;
    skinned.slot_materials = vec![Some(material)];
    let hip = object(&mut world, 0, Vec3::new(1.0, 1.0, 0.0), Quat::IDENTITY);
    world
        .set_parent(hip, Some(rig), ParentSpace::KeepLocal)
        .unwrap();
    let pose = world.world_matrix(hip).unwrap().inverse() * world.world_matrix(body).unwrap();
    skinned.skin = SkinBinding {
        drawn_mesh: MESH,
        root: Some(SkinRoot {
            bone: hip,
            bind_pose: pose.to_cols_array(),
        }),
    };
    world
        .add_component(body, ComponentDesc::MeshRenderer(skinned))
        .unwrap();
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert!(
        adapted.report.renderers_skipped.is_empty(),
        "{:?}",
        adapted.report.renderers_skipped
    );
    let drawn = &adapted.frame.renderers[&RendererId(0)];
    assert_eq!(drawn.mesh, MeshId(MESH));
    let placed = matrix(&drawn.per_draw, "unity_ObjectToWorld");
    assert_close(&placed, &world.world_matrix(body).unwrap().to_cols_array());
}

#[test]
fn a_skinned_renderer_with_no_root_bone_is_drawn_nowhere_and_reported() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let shader = shader(&mut world, "Draw");
    let material = material(&mut world, shader);
    let body = object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let mut skinned = MeshRenderer::of_kind(RendererKind::Skinned);
    skinned.material_count = 1;
    skinned.slot_materials = vec![Some(material)];
    skinned.skin = SkinBinding {
        drawn_mesh: MESH,
        root: None,
    };
    world
        .add_component(body, ComponentDesc::MeshRenderer(skinned))
        .unwrap();
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());
    assert!(adapted.frame.renderers.is_empty());
    assert_eq!(
        adapted.report.renderers_skipped,
        std::collections::BTreeMap::from([(RendererSkip::SkinnedNotDrawn, 1)])
    );
}

#[test]
fn blend_weights_and_indices_are_channels_by_their_number_of_four_floats_each() {
    let mut stored = mesh();
    stored
        .channels
        .push(channel(12, 4, (0..12_u8).map(f32::from).collect()));
    stored.channels.push(channel(
        13,
        4,
        (0..12_u8).map(|v| 100.0 + f32::from(v)).collect(),
    ));
    let made = mesh_of(&stored).unwrap();
    assert_eq!(sources(&made), [0, 4, 12, 13]);
    assert_eq!(made.channels[2].format, ChannelFormat::Float32x4);
    assert_eq!(made.channels[3].format, ChannelFormat::Float32x4);
    let expected = |first: u8| (0..12_u8).map(|v| f32::from(first + v)).collect::<Vec<_>>();
    assert_eq!(floats(&made.channels[2].bytes), expected(0));
    assert_eq!(floats(&made.channels[3].bytes), expected(100));
}
