//! A skinned renderer reaches the frame like any other renderer: the world lists it at the matrix
//! of the object that holds its animator and the adapter draws it, with its mesh's blend channels
//! available to a pass that reads them.

use glam::{Quat, Vec3};
use worldrunner_render_program::ids::{MeshId, RendererId};
use worldrunner_world_simulation::graphics::OffscreenWork;
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, MeshRenderer, ParentSpace, RendererKind, SkinBinding,
};

use super::super::frame::adapt;
use super::frame::settings;
use super::support::{
    assert_close, available, camera, material, matrix, object, render_texture, shader, snapshot,
    world,
};

const MESH: u64 = (1 << 41) + 7;

#[test]
fn a_skinned_renderer_is_drawn_at_its_rigs_matrix_and_no_renderer_is_skipped_for_being_skinned() {
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
    skinned.skin = SkinBinding {
        drawn_mesh: MESH,
        bones: vec![Some(rig), Some(body)],
        root_bone: Some(rig),
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
    assert_close(
        &placed,
        &glam::Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)).to_cols_array(),
    );
}
