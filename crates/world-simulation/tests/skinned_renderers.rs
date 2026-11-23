//! A skinned mesh renderer is not drawn yet, and the snapshot counts the ones a camera render
//! would have drawn so that a host can say what it leaves out.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::snapshot_wire::{decode, encode};
use worldrunner_world_simulation::graphics::{GraphicsSnapshot, OffscreenWork};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentHandle, MeshRenderer, RendererKind, RendererSwitches, World,
};

mod graphics_world;
use graphics_world::{
    camera, material, render_texture, renderer, renderer_with, shader, snapshot, world,
};

const MESH: u64 = (1 << 41) + 9;

fn skinned() -> MeshRenderer {
    MeshRenderer::of_kind(RendererKind::Skinned)
}

fn render(world: &mut World, camera: ComponentHandle) {
    let target = render_texture(world, [8, 8]);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
}

/// A world with one mesh renderer and the skinned `renderers`, all on layer 0, drawn by one camera.
fn drawn_with(renderers: Vec<MeshRenderer>) -> GraphicsSnapshot {
    let mut world = world();
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let lit = shader(&mut world, "Lit");
    let surface = material(&mut world, lit);
    renderer(&mut world, 0, MESH, vec![Some(surface)]);
    for (at, renderer) in renderers.into_iter().enumerate() {
        renderer_with(&mut world, (0, true), MESH + 1 + at as u64, renderer);
    }
    render(&mut world, camera);
    snapshot(&mut world)
}

#[test]
fn a_skinned_renderer_is_counted_and_the_mesh_renderer_still_draws() {
    let snapshot = drawn_with(vec![skinned()]);
    assert_eq!(snapshot.skinned_not_drawn, 1);
    let meshes: Vec<u64> = snapshot.renderers.iter().map(|r| r.mesh).collect();
    assert_eq!(meshes, [MESH]);
}

#[test]
fn a_skinned_renderer_that_is_off_or_on_another_layer_is_not_counted() {
    let off = MeshRenderer {
        switches: RendererSwitches {
            enabled: false,
            ..RendererSwitches::default()
        },
        ..skinned()
    };
    let forced_off = MeshRenderer {
        switches: RendererSwitches {
            force_rendering_off: true,
            ..RendererSwitches::default()
        },
        ..skinned()
    };
    assert_eq!(drawn_with(vec![off, forced_off]).skinned_not_drawn, 0);

    let mut world = world();
    let camera = camera(
        &mut world,
        Camera {
            culling_mask: 1,
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    renderer_with(&mut world, (5, true), MESH, skinned());
    renderer_with(&mut world, (0, false), MESH + 1, skinned());
    render(&mut world, camera);
    assert_eq!(snapshot(&mut world).skinned_not_drawn, 0);
}

#[test]
fn a_skinned_renderer_seen_by_two_renders_is_counted_once() {
    let mut world = world();
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    renderer_with(&mut world, (0, true), MESH, skinned());
    render(&mut world, camera);
    render(&mut world, camera);
    assert_eq!(snapshot(&mut world).skinned_not_drawn, 1);
}

#[test]
fn a_frame_with_no_skinned_renderers_counts_none_and_the_next_frame_starts_over() {
    let mut world = world();
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    renderer_with(&mut world, (0, true), MESH, skinned());
    render(&mut world, camera);
    assert_eq!(snapshot(&mut world).skinned_not_drawn, 1);
    assert_eq!(snapshot(&mut world).skinned_not_drawn, 0);
}

#[test]
fn the_count_crosses_the_wire() {
    let snapshot = drawn_with(vec![skinned(), skinned()]);
    assert_eq!(snapshot.skinned_not_drawn, 2);
    let decoded = decode(&encode(&snapshot).unwrap()).unwrap();
    assert_eq!(decoded.skinned_not_drawn, 2);
}
