//! What the graphics snapshot copies out of a world.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::snapshot_wire::{decode, encode};
use worldrunner_world_simulation::graphics::{
    ComputeKernel, OffscreenWork, ShaderSource, SkinView, TargetRef, TextureRef, WorldComputeShader,
};
use worldrunner_world_simulation::presentation::{
    Camera, Light, LightBakeType, LightKind, LightSettings, LightShadowMode,
    mesh_objects::extract_mesh_objects,
};
use worldrunner_world_simulation::{ComponentDesc, ObjectHandle, ParentSpace, World};
use worldrunner_world_simulation::{MeshRenderer, RendererSwitches};
use worldrunner_world_simulation::{RendererKind, SkinBinding};

mod graphics_world;
use graphics_world::{
    camera, material, object, render_texture, renderer, renderer_with, shader, snapshot, world,
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
    assert!(second.cameras.is_empty());
}

#[test]
fn a_frame_with_no_work_still_lists_every_live_render_texture() {
    let mut world = world();
    let kept = render_texture(&mut world, [8, 8]);
    let destroyed = render_texture(&mut world, [4, 4]);
    world.destroy_render_texture(destroyed).unwrap();
    let idle = snapshot(&mut world);
    assert!(idle.work.is_empty());
    assert_eq!(
        idle.render_textures.keys().copied().collect::<Vec<_>>(),
        [kept],
        "a host retains what the world holds, so the list is whole whatever the work is"
    );
    assert_eq!(
        idle.render_textures[&kept],
        *world.render_texture(kept).unwrap()
    );
    assert_eq!(decode(&encode(&idle).unwrap()).unwrap(), idle);
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
            target: TargetRef::Render(target),
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

fn compute_shader(world: &mut worldrunner_world_simulation::World, name: &str) -> u64 {
    let id = world
        .create_compute_shader(WorldComputeShader {
            name: name.into(),
            source: ShaderSource {
                file: "resources.assets".into(),
                path_id: 8167,
            },
            kernels: vec![ComputeKernel {
                name: "Main".into(),
                thread_group_size: Some([8, 8, 1]),
            }],
            generation: 0,
        })
        .unwrap();
    id.to_bits()
}

#[test]
fn a_dispatch_brings_the_compute_shader_and_the_buffers_it_names() {
    let mut world = world();
    let used = compute_shader(&mut world, "Used");
    let unused = compute_shader(&mut world, "Unused");
    let bound = world.create_compute_buffer(4, 4, 0).unwrap();
    let other = world.create_compute_buffer(2, 8, 0).unwrap();
    let id = worldrunner_world_simulation::graphics::ComputeShaderId::from_bits(used);
    world
        .bind_compute_buffer(id, 0, "_Output".into(), bound)
        .unwrap();
    world
        .set_compute_buffer_data(bound, 0, &[1, 2, 3, 4])
        .unwrap();
    world.dispatch_compute(id, 0, [2, 1, 1]).unwrap();
    let snapshot = snapshot(&mut world);
    let shaders: Vec<u64> = snapshot
        .compute_shaders
        .keys()
        .map(|id| id.to_bits())
        .collect();
    assert_eq!(
        shaders,
        [used],
        "the shader {unused} no dispatch names is left out"
    );
    assert_eq!(
        snapshot.compute_shaders.values().next().unwrap().name,
        "Used"
    );
    let buffers: Vec<_> = snapshot.compute_buffers.keys().copied().collect();
    assert_eq!(
        buffers,
        [bound],
        "the buffer {other:?} no dispatch binds is left out"
    );
    assert_eq!(snapshot.compute_buffers[&bound].contents[..4], [1, 2, 3, 4]);
}

#[test]
fn a_compute_shader_or_buffer_destroyed_before_the_take_is_left_out() {
    let mut world = world();
    let shader = compute_shader(&mut world, "Gone");
    let id = worldrunner_world_simulation::graphics::ComputeShaderId::from_bits(shader);
    let buffer = world.create_compute_buffer(1, 4, 0).unwrap();
    world
        .bind_compute_buffer(id, 0, "_Output".into(), buffer)
        .unwrap();
    world.dispatch_compute(id, 0, [1, 1, 1]).unwrap();
    world.destroy_compute_buffer(buffer).unwrap();
    world.destroy_compute_shader(id).unwrap();
    let snapshot = snapshot(&mut world);
    assert_eq!(snapshot.work.len(), 1, "the record stays in the list");
    assert!(snapshot.compute_shaders.is_empty() && snapshot.compute_buffers.is_empty());
}

#[test]
fn a_frame_without_dispatches_copies_no_compute_resources() {
    let mut world = world();
    compute_shader(&mut world, "Idle");
    world.create_compute_buffer(1, 4, 0).unwrap();
    let target = render_texture(&mut world, [8, 8]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::None,
            target: TargetRef::Render(target),
            material: None,
            pass: -1,
        })
        .unwrap();
    let snapshot = snapshot(&mut world);
    assert_eq!(snapshot.work.len(), 1);
    assert!(snapshot.compute_shaders.is_empty() && snapshot.compute_buffers.is_empty());
}

/// A light whose every setting differs from a new light's.
fn settings(kind: LightKind) -> LightSettings {
    LightSettings {
        kind,
        color: [0.25, 0.5, 0.75, 0.5],
        intensity: 2.5,
        range: 7.0,
        spot_angle: 40.0,
        inner_spot_angle: 12.0,
        shadows: LightShadowMode::Soft,
        shadow_strength: 0.75,
        shadow_bias: 0.125,
        shadow_normal_bias: 0.25,
        shadow_near_plane: 0.5,
        culling_mask: 0b1010,
        rendering_layer_mask: 3,
        bounce_intensity: 0.5,
        color_temperature: 5000.0,
        use_color_temperature: true,
        cookie_size: 2.0,
        cookie_texture: (1 << 40) + 3,
        bake_type: LightBakeType::Mixed,
        shadow_resolution: 2,
    }
}

fn light(world: &mut World, enabled: bool, settings: LightSettings, at: Vec3) -> ObjectHandle {
    let object = object(world, 0, at, Quat::from_rotation_x(0.5));
    world
        .add_component(object, ComponentDesc::Light(Light { enabled, settings }))
        .unwrap();
    object
}

/// A world with a camera render recorded, so the snapshot has a camera to light.
fn rendering_world() -> World {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    world
}

#[test]
fn a_camera_render_copies_the_enabled_lights_of_active_objects_in_component_order() {
    let mut world = rendering_world();
    let directional = settings(LightKind::Directional);
    let point = settings(LightKind::Point);
    light(&mut world, true, directional, Vec3::new(1.0, 2.0, 3.0));
    light(&mut world, false, settings(LightKind::Spot), Vec3::ZERO);
    let hidden = light(&mut world, true, settings(LightKind::Spot), Vec3::ZERO);
    world.set_active(hidden, false).unwrap();
    light(&mut world, true, point, Vec3::new(-4.0, 0.0, 0.0));
    let lights = snapshot(&mut world).lights;
    assert_eq!(
        lights.len(),
        2,
        "the disabled light and the inactive one are left out"
    );
    assert_eq!(lights[0].settings, directional);
    assert_eq!(lights[1].settings, point);
    // Column-major: the translation is the last column, in the world's own coordinates.
    assert_eq!(lights[0].world_matrix[12..15], [1.0, 2.0, 3.0]);
    let forward = Quat::from_rotation_x(0.5) * Vec3::Z;
    for (column, expected) in lights[0].world_matrix[8..11].iter().zip(forward.to_array()) {
        assert!(
            (column - expected).abs() < 1e-6,
            "the third column is the forward axis"
        );
    }
}

#[test]
fn a_frame_without_camera_work_copies_no_lights() {
    let mut world = world();
    light(
        &mut world,
        true,
        settings(LightKind::Directional),
        Vec3::ZERO,
    );
    let source = render_texture(&mut world, [4, 4]);
    let target = render_texture(&mut world, [4, 4]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(target),
            material: None,
            pass: -1,
        })
        .unwrap();
    let taken = snapshot(&mut world);
    assert_eq!(taken.work.len(), 1);
    assert!(taken.lights.is_empty());
}

#[test]
fn lights_round_trip_through_the_byte_encoding_exactly() {
    let mut world = rendering_world();
    for kind in [LightKind::Directional, LightKind::Tube, LightKind::Spot] {
        light(&mut world, true, settings(kind), Vec3::new(0.5, -0.0, 9.0));
    }
    let taken = snapshot(&mut world);
    assert_eq!(taken.lights.len(), 3);
    assert_eq!(decode(&encode(&taken).unwrap()).unwrap(), taken);
}

/// An object under `parent` at `position` that holds an animator.
fn animated(world: &mut World, parent: Option<ObjectHandle>, position: Vec3) -> ObjectHandle {
    let object = object(world, 0, position, Quat::IDENTITY);
    world
        .set_parent(object, parent, ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(object, ComponentDesc::Animator { enabled: true })
        .unwrap();
    object
}

/// Whether two column-major matrices agree to within rounding.
fn same_matrix(a: [f32; 16], b: [f32; 16]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

/// An object under `parent` at `position`, turned by `rotation`, holding a skinned renderer with
/// one material slot that draws `mesh` and names `bones`.
fn skinned(
    world: &mut World,
    parent: Option<ObjectHandle>,
    (position, rotation): (Vec3, Quat),
    (mesh, bones): (u64, Vec<Option<ObjectHandle>>),
) -> ObjectHandle {
    let object = object(world, 0, position, rotation);
    world
        .set_parent(object, parent, ParentSpace::KeepLocal)
        .unwrap();
    let mut renderer = MeshRenderer::of_kind(RendererKind::Skinned);
    renderer.material_count = 1;
    renderer.skin = SkinBinding {
        drawn_mesh: mesh,
        bones,
        root_bone: None,
    };
    world
        .add_component(object, ComponentDesc::MeshRenderer(renderer))
        .unwrap();
    object
}

/// The one renderer a frame drawing every layer lists, after a camera render of `world`.
fn drawn(world: &mut World) -> worldrunner_world_simulation::graphics::GraphicsSnapshot {
    let target = render_texture(world, [8, 8]);
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    snapshot(world)
}

fn quarter_turn() -> Quat {
    Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)
}

#[test]
fn a_skinned_renderer_is_placed_at_the_matrix_of_its_animator_not_its_own() {
    let mut world = world();
    let rig = animated(&mut world, None, Vec3::new(5.0, 0.0, 0.0));
    let body = skinned(
        &mut world,
        Some(rig),
        (Vec3::new(0.0, 2.0, 0.0), quarter_turn()),
        (MESH, vec![Some(rig), None]),
    );
    let snapshot = drawn(&mut world);
    let [view] = &snapshot.renderers[..] else {
        panic!("one renderer: {:?}", snapshot.renderers)
    };
    assert_eq!(view.mesh, MESH);
    assert!(
        same_matrix(
            view.local_to_world,
            glam::Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)).to_cols_array()
        ),
        "the rig's matrix, not the renderer's rotation and offset"
    );
    assert_eq!(view.skin, Some(SkinView { bone_count: 2 }));
    assert!(!same_matrix(
        world.world_matrix(body).unwrap().to_cols_array(),
        view.local_to_world
    ));
}

#[test]
fn a_skinned_renderer_without_an_animator_above_it_keeps_its_own_matrix() {
    let mut world = world();
    let parent = object(&mut world, 0, Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY);
    let body = skinned(
        &mut world,
        Some(parent),
        (Vec3::new(0.0, 3.0, 0.0), quarter_turn()),
        (MESH, vec![]),
    );
    let snapshot = drawn(&mut world);
    assert!(same_matrix(
        snapshot.renderers[0].local_to_world,
        world.world_matrix(body).unwrap().to_cols_array()
    ));
    assert_eq!(snapshot.renderers[0].skin, Some(SkinView { bone_count: 0 }));
}

#[test]
fn the_nearest_animator_above_a_skinned_renderer_places_it() {
    let mut world = world();
    let outer = animated(&mut world, None, Vec3::new(1.0, 0.0, 0.0));
    let inner = animated(&mut world, Some(outer), Vec3::new(0.0, 4.0, 0.0));
    skinned(
        &mut world,
        Some(inner),
        (Vec3::new(7.0, 7.0, 7.0), Quat::IDENTITY),
        (MESH, vec![]),
    );
    let snapshot = drawn(&mut world);
    let place = &snapshot.renderers[0].local_to_world[12..15];
    assert!(
        place
            .iter()
            .zip([1.0, 4.0, 0.0])
            .all(|(a, b)| (a - b).abs() < 1e-5),
        "the inner animator, itself under the outer one: {place:?}"
    );
}

#[test]
fn a_mesh_renderer_has_no_skin_and_a_skinned_one_draws_its_own_mesh_not_a_filters() {
    let mut world = world();
    renderer(&mut world, 0, MESH, vec![]);
    let body = skinned(
        &mut world,
        None,
        (Vec3::ZERO, Quat::IDENTITY),
        (MESH + 1, vec![]),
    );
    world
        .add_component(
            body,
            ComponentDesc::MeshFilter(worldrunner_world_simulation::MeshFilter {
                mesh: 3,
                drawn_mesh: MESH + 2,
            }),
        )
        .unwrap();
    let snapshot = drawn(&mut world);
    let meshes: Vec<(u64, bool)> = snapshot
        .renderers
        .iter()
        .map(|view| (view.mesh, view.skin.is_some()))
        .collect();
    assert_eq!(meshes, [(MESH, false), (MESH + 1, true)]);
}

#[test]
fn a_skinned_renderer_that_names_no_drawn_mesh_is_not_copied() {
    let mut world = world();
    skinned(&mut world, None, (Vec3::ZERO, Quat::IDENTITY), (0, vec![]));
    assert!(drawn(&mut world).renderers.is_empty());
}
