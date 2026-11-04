//! What the snapshot copies out of a world.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::{OffscreenWork, TextureRef};
use worldrunner_world_simulation::presentation::{Camera, mesh_objects::extract_mesh_objects};
use worldrunner_world_simulation::{MeshRenderer, RendererSwitches};

use super::support::{
    camera, material, render_texture, renderer, renderer_with, shader, snapshot, world,
};

const MESH: u64 = (1 << 41) + 9;

#[test]
fn taking_drains_the_records_and_the_next_frame_has_none() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let first = snapshot(&mut world);
    assert_eq!(first.work.len(), 1);
    assert!(first.cameras.contains_key(&camera));
    let second = snapshot(&mut world);
    assert!(second.work.is_empty());
    assert!(second.cameras.is_empty() && second.render_textures.is_empty());
}

#[test]
fn a_camera_records_its_settings_and_where_it_stands() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let settings = Camera {
        culling_mask: 0b100,
        ..Camera::default()
    };
    let position = Vec3::new(1.0, 2.0, 3.0);
    let camera = camera(&mut world, settings, position, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let view = &snapshot(&mut world).cameras[&camera];
    assert_eq!(view.settings.culling_mask, 0b100);
    // Column-major: the translation is the last column.
    assert_eq!(view.world_matrix[12..15], [1.0, 2.0, 3.0]);
}

#[test]
fn renderers_on_layers_the_screen_camera_does_not_draw_reach_the_offscreen_camera() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    // The screen camera draws layer 0 only; the offscreen one draws layer 5.
    camera(
        &mut world,
        Camera {
            culling_mask: 1,
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let offscreen = camera(
        &mut world,
        Camera {
            culling_mask: 1 << 5,
            target_texture: Some(target),
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    renderer(&mut world, 5, MESH, vec![]);
    assert!(
        extract_mesh_objects(&world).unwrap().is_empty(),
        "the presentation leaves the layer out"
    );
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera: offscreen,
            target,
        })
        .unwrap();
    let snapshot = snapshot(&mut world);
    assert_eq!(snapshot.renderers.len(), 1);
    assert_eq!(snapshot.renderers[0].layer, 5);
    assert_eq!(snapshot.renderers[0].mesh, MESH);
}

#[test]
fn renderers_that_no_camera_of_the_frame_can_draw_are_not_copied() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(
        &mut world,
        Camera {
            culling_mask: 1 << 2,
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    renderer(&mut world, 2, MESH, vec![]);
    renderer(&mut world, 3, MESH + 1, vec![]);
    renderer(&mut world, 2, MESH + 2, vec![]);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let meshes: Vec<u64> = snapshot.renderers.iter().map(|r| r.mesh).collect();
    assert_eq!(meshes, [MESH, MESH + 2], "layer 3 is outside the mask");
}

#[test]
fn a_renderer_that_is_off_or_on_an_inactive_object_is_not_copied() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let forced_off = MeshRenderer {
        switches: RendererSwitches {
            force_rendering_off: true,
            ..RendererSwitches::default()
        },
        ..MeshRenderer::default()
    };
    let disabled = MeshRenderer {
        switches: RendererSwitches {
            enabled: false,
            ..RendererSwitches::default()
        },
        ..MeshRenderer::default()
    };
    renderer_with(&mut world, (0, true), MESH, forced_off);
    renderer_with(&mut world, (0, true), MESH + 1, disabled);
    renderer_with(&mut world, (0, false), MESH + 2, MeshRenderer::default());
    renderer_with(&mut world, (0, true), MESH + 3, MeshRenderer::default());
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let meshes: Vec<u64> = snapshot(&mut world)
        .renderers
        .iter()
        .map(|r| r.mesh)
        .collect();
    assert_eq!(meshes, [MESH + 3]);
}

#[test]
fn materials_and_shaders_of_blits_and_renderers_are_copied_and_destroyed_ones_are_not() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let blit_shader = shader(&mut world, "Blit");
    let draw_shader = shader(&mut world, "Draw");
    let unused_shader = shader(&mut world, "Unused");
    let blit = material(&mut world, blit_shader);
    let drawn = material(&mut world, draw_shader);
    let destroyed = material(&mut world, draw_shader);
    material(&mut world, unused_shader);
    renderer(&mut world, 0, MESH, vec![Some(drawn), Some(destroyed)]);
    world.destroy_material(destroyed).unwrap();
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(target),
            target,
            material: Some(blit),
            pass: 0,
        })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let materials: Vec<_> = snapshot.materials.keys().copied().collect();
    assert_eq!(materials, [blit, drawn]);
    let shaders: Vec<_> = snapshot.shaders.keys().copied().collect();
    assert_eq!(shaders, [blit_shader, draw_shader]);
    assert_eq!(
        snapshot.renderers[0].materials,
        [Some(drawn), None],
        "a destroyed material is a slot with none"
    );
}
