//! A camera render draws the renderers that are drawable when the script calls it, not the ones
//! that are drawable when the frame's snapshot is taken.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::snapshot_wire::{decode, encode};
use worldrunner_world_simulation::graphics::{GraphicsSnapshot, OffscreenWork, RenderTextureId};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, MeshFilter, MeshRenderer, ObjectDesc, ObjectHandle,
    ParentSpace, RendererSwitches, World,
};

mod graphics_world;
use graphics_world::{
    camera, material, render_texture, renderer, renderer_with, shader, snapshot, world,
};

const MESH: u64 = (1 << 41) + 9;

struct Scene {
    world: World,
    camera: ComponentHandle,
    target: RenderTextureId,
}

fn scene() -> Scene {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    Scene {
        world,
        camera,
        target,
    }
}

impl Scene {
    fn render(&mut self) {
        let (camera, target) = (self.camera, self.target);
        self.world
            .record_offscreen(OffscreenWork::CameraRender { camera, target })
            .unwrap();
    }

    fn mesh_renderer(&self, object: ObjectHandle) -> ComponentHandle {
        self.world
            .published_components()
            .find(|c| c.object == object && matches!(c.desc, ComponentDesc::MeshRenderer(_)))
            .map(|c| c.handle)
            .unwrap()
    }

    fn switch(&mut self, object: ObjectHandle, switches: RendererSwitches) {
        let handle = self.mesh_renderer(object);
        let renderer = MeshRenderer {
            switches,
            ..MeshRenderer::default()
        };
        self.world
            .set_component(handle, ComponentDesc::MeshRenderer(renderer))
            .unwrap();
    }
}

/// The meshes the camera render at `record` draws.
fn drawn_by(snapshot: &GraphicsSnapshot, record: usize) -> Option<Vec<u64>> {
    let set = snapshot.record_renderers(record)?;
    let mut meshes: Vec<u64> = set
        .iter()
        .map(|&index| snapshot.renderers[index as usize].mesh)
        .collect();
    meshes.sort_unstable();
    Some(meshes)
}

#[test]
fn a_renderer_that_is_active_only_around_the_render_is_in_its_set() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, false), MESH, MeshRenderer::default());
    scene.world.set_active(object, true).unwrap();
    scene.render();
    scene.world.set_active(object, false).unwrap();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![MESH]));
}

#[test]
fn a_renderer_that_is_deactivated_after_the_render_stays_in_its_set() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, true), MESH, MeshRenderer::default());
    scene.render();
    scene.world.set_active(object, false).unwrap();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![MESH]));
}

#[test]
fn a_renderer_that_is_destroyed_after_the_render_leaves_its_set() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, true), MESH, MeshRenderer::default());
    renderer_with(
        &mut scene.world,
        (0, true),
        MESH + 1,
        MeshRenderer::default(),
    );
    scene.render();
    scene.world.destroy_object(object).unwrap();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![MESH + 1]));
}

#[test]
fn a_renderer_that_is_activated_after_the_render_is_not_in_its_set() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, false), MESH, MeshRenderer::default());
    scene.render();
    scene.world.set_active(object, true).unwrap();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![]));
    assert!(
        snapshot.renderers.is_empty(),
        "no record draws it, so the snapshot does not list it"
    );
}

#[test]
fn a_renderer_that_is_off_when_the_render_is_called_is_not_in_its_set() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, true), MESH, MeshRenderer::default());
    let disabled = RendererSwitches {
        enabled: false,
        ..RendererSwitches::default()
    };
    scene.switch(object, disabled);
    scene.render();
    scene.switch(object, RendererSwitches::default());
    let forced_off = RendererSwitches {
        force_rendering_off: true,
        ..RendererSwitches::default()
    };
    scene.switch(object, forced_off);
    scene.render();
    scene.switch(object, RendererSwitches::default());
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![]));
    assert_eq!(drawn_by(&snapshot, 1), Some(vec![]));
}

#[test]
fn a_renderer_that_is_drawable_at_the_render_is_in_its_set_after_it_is_switched_off() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, true), MESH, MeshRenderer::default());
    scene.render();
    let disabled = RendererSwitches {
        enabled: false,
        ..RendererSwitches::default()
    };
    scene.switch(object, disabled);
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![MESH]));
}

#[test]
fn a_renderer_on_a_layer_the_camera_does_not_draw_is_not_in_its_set() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let settings = Camera {
        culling_mask: 1 << 2,
        ..Camera::default()
    };
    let camera = camera(&mut world, settings, Vec3::ZERO, Quat::IDENTITY);
    let mut scene = Scene {
        world,
        camera,
        target,
    };
    renderer_with(&mut scene.world, (3, true), MESH, MeshRenderer::default());
    renderer_with(
        &mut scene.world,
        (2, true),
        MESH + 1,
        MeshRenderer::default(),
    );
    scene.render();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![MESH + 1]));
}

#[test]
fn moving_a_renderer_onto_a_layer_the_camera_draws_puts_it_in_the_later_set_only() {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let settings = Camera {
        culling_mask: 1 << 2,
        ..Camera::default()
    };
    let camera = camera(&mut world, settings, Vec3::ZERO, Quat::IDENTITY);
    let mut scene = Scene {
        world,
        camera,
        target,
    };
    let object = renderer_with(&mut scene.world, (3, true), MESH, MeshRenderer::default());
    scene.render();
    scene.world.set_layer(object, 2).unwrap();
    scene.render();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![]));
    assert_eq!(drawn_by(&snapshot, 1), Some(vec![MESH]));
}

#[test]
fn a_renderer_whose_mesh_filter_loses_its_mesh_is_out_of_the_later_set() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, true), MESH, MeshRenderer::default());
    scene.render();
    let filter = scene
        .world
        .published_components()
        .find(|c| c.object == object && matches!(c.desc, ComponentDesc::MeshFilter(_)))
        .map(|c| c.handle)
        .unwrap();
    let empty = MeshFilter {
        mesh: 3,
        drawn_mesh: 0,
    };
    scene
        .world
        .set_component(filter, ComponentDesc::MeshFilter(empty))
        .unwrap();
    scene.render();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(
        drawn_by(&snapshot, 0),
        Some(vec![]),
        "the mesh is read as of now"
    );
    assert_eq!(drawn_by(&snapshot, 1), Some(vec![]));
    assert_eq!(snapshot.renderer_sets.len(), 2);
}

#[test]
fn records_with_nothing_changed_between_them_share_one_set() {
    let mut scene = scene();
    renderer(&mut scene.world, 0, MESH, vec![]);
    scene.render();
    scene.render();
    scene.render();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(snapshot.renderer_sets.len(), 1);
    assert_eq!(snapshot.record_sets.len(), 3);
    for record in 0..3 {
        assert_eq!(drawn_by(&snapshot, record), Some(vec![MESH]));
    }
}

#[test]
fn a_change_between_two_records_gives_each_its_own_set() {
    type Change = fn(&mut Scene, ObjectHandle, ObjectHandle);
    let changes: [(&str, Change, bool); 6] = [
        (
            "the object is deactivated",
            |scene, object, _| {
                scene.world.set_active(object, false).unwrap();
            },
            true,
        ),
        (
            "the parent is deactivated",
            |scene, _, parent| {
                scene.world.set_active(parent, false).unwrap();
            },
            true,
        ),
        (
            "the object moves under an inactive parent",
            |scene, object, _| {
                let inactive = ObjectDesc {
                    active: false,
                    ..ObjectDesc::default()
                };
                let inactive = scene.world.create_object(inactive).unwrap();
                scene
                    .world
                    .set_parent(object, Some(inactive), ParentSpace::KeepLocal)
                    .unwrap();
            },
            true,
        ),
        (
            "the renderer is disabled",
            |scene, object, _| {
                let disabled = RendererSwitches {
                    enabled: false,
                    ..RendererSwitches::default()
                };
                scene.switch(object, disabled);
            },
            true,
        ),
        (
            "the renderer is forced off",
            |scene, object, _| {
                let forced_off = RendererSwitches {
                    force_rendering_off: true,
                    ..RendererSwitches::default()
                };
                scene.switch(object, forced_off);
            },
            true,
        ),
        (
            "the object is destroyed",
            |scene, object, _| {
                scene.world.destroy_object(object).unwrap();
            },
            false,
        ),
    ];
    for (name, change, survives) in changes {
        let mut scene = scene();
        let parent = scene.world.create_object(ObjectDesc::default()).unwrap();
        let object = renderer_with(&mut scene.world, (0, true), MESH, MeshRenderer::default());
        scene
            .world
            .set_parent(object, Some(parent), ParentSpace::KeepLocal)
            .unwrap();
        scene.render();
        change(&mut scene, object, parent);
        scene.render();
        let snapshot = snapshot(&mut scene.world);
        let first = if survives { vec![MESH] } else { vec![] };
        assert_eq!(drawn_by(&snapshot, 0), Some(first), "{name}");
        assert_eq!(drawn_by(&snapshot, 1), Some(vec![]), "{name}");
        assert_eq!(snapshot.renderer_sets.len(), 2, "{name}");
    }
}

#[test]
fn a_renderer_that_becomes_drawable_between_two_records_is_only_in_the_later_set() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, false), MESH, MeshRenderer::default());
    scene.render();
    scene.world.set_active(object, true).unwrap();
    scene.render();
    scene.world.set_active(object, false).unwrap();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![]));
    assert_eq!(drawn_by(&snapshot, 1), Some(vec![MESH]));
}

#[test]
fn a_culling_record_has_the_set_of_renderers_that_were_drawable_when_it_was_recorded() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, false), MESH, MeshRenderer::default());
    scene.world.set_active(object, true).unwrap();
    scene
        .world
        .record_offscreen(OffscreenWork::Cull {
            id: 1,
            camera: scene.camera,
            culling_mask: 1,
            planes: vec![],
            options: 0,
        })
        .unwrap();
    scene.world.set_active(object, false).unwrap();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![MESH]));
    assert_eq!(snapshot.renderers.len(), 1);
}

#[test]
fn a_culling_uses_its_own_mask_and_not_its_cameras() {
    let mut scene = scene();
    renderer_with(&mut scene.world, (2, true), MESH, MeshRenderer::default());
    scene
        .world
        .record_offscreen(OffscreenWork::Cull {
            id: 1,
            camera: scene.camera,
            culling_mask: 1 << 2,
            planes: vec![],
            options: 0,
        })
        .unwrap();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(drawn_by(&snapshot, 0), Some(vec![MESH]));
}

#[test]
fn a_renderer_in_a_set_that_is_active_at_the_snapshot_is_listed_once() {
    let mut scene = scene();
    renderer(&mut scene.world, 0, MESH, vec![]);
    scene.render();
    scene.render();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(snapshot.renderers.len(), 1);
}

#[test]
fn the_materials_of_a_renderer_only_a_set_lists_are_copied() {
    let mut scene = scene();
    let shader = shader(&mut scene.world, "Bake");
    let material = material(&mut scene.world, shader);
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        ..MeshRenderer::default()
    };
    let object = renderer_with(&mut scene.world, (0, false), MESH, renderer);
    scene.world.set_active(object, true).unwrap();
    scene.render();
    scene.world.set_active(object, false).unwrap();
    let snapshot = snapshot(&mut scene.world);
    assert_eq!(snapshot.renderers[0].materials, vec![Some(material)]);
    assert!(snapshot.materials.contains_key(&material));
    assert!(
        snapshot
            .shaders
            .contains_key(&snapshot.materials[&material].shader)
    );
}

#[test]
fn the_sets_cross_the_wire() {
    let mut scene = scene();
    let object = renderer_with(&mut scene.world, (0, false), MESH, MeshRenderer::default());
    scene.world.set_active(object, true).unwrap();
    scene.render();
    scene.world.set_active(object, false).unwrap();
    scene.render();
    let snapshot = snapshot(&mut scene.world);
    let decoded = decode(&encode(&snapshot).unwrap()).unwrap();
    assert_eq!(decoded, snapshot);
    assert_eq!(drawn_by(&decoded, 0), Some(vec![MESH]));
    assert_eq!(drawn_by(&decoded, 1), Some(vec![]));
}

#[test]
fn the_next_frame_starts_without_the_sets_of_the_last() {
    let mut scene = scene();
    renderer_with(&mut scene.world, (0, true), MESH, MeshRenderer::default());
    scene.render();
    let first = snapshot(&mut scene.world);
    assert_eq!(first.renderer_sets.len(), 1);
    scene.render();
    let second = snapshot(&mut scene.world);
    assert_eq!(second.renderer_sets.len(), 1);
    assert_eq!(drawn_by(&second, 0), Some(vec![MESH]));
}
