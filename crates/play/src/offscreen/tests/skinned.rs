//! A skinned mesh renderer that the snapshot leaves out because skinned meshes are not drawn yet
//! is counted by name in the frame's report, and the mesh renderers still draw.

use glam::{Quat, Vec3};
use worldrunner_render_program::ids::RendererId;
use worldrunner_world_simulation::graphics::OffscreenWork;
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{MeshRenderer, RendererKind};

use super::super::frame::{RendererSkip, adapt};
use super::frame::settings;
use super::support::{
    available, camera, material, render_texture, renderer, renderer_with, shader, snapshot, world,
};

const MESH: u64 = (1 << 41) + 3;

#[test]
fn a_skinned_renderer_is_a_named_skip_and_the_mesh_renderer_still_draws() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let shader = shader(&mut world, "Draw");
    let drawn = material(&mut world, shader);
    renderer(&mut world, 0, MESH, vec![Some(drawn)]);
    let skinned = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(drawn)],
        ..MeshRenderer::of_kind(RendererKind::Skinned)
    };
    renderer_with(&mut world, (0, true), MESH + 1, skinned);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let snapshot = snapshot(&mut world);
    let adapted = adapt(&snapshot, &available(&snapshot, 1, &[]), &settings());

    let listed: Vec<_> = adapted.frame.renderers.keys().copied().collect();
    assert_eq!(listed, [RendererId(0)]);
    assert_eq!(
        adapted
            .report
            .renderers_skipped
            .iter()
            .map(|(k, n)| (*k, *n))
            .collect::<Vec<_>>(),
        [(RendererSkip::SkinnedNotDrawn, 1)]
    );
}
