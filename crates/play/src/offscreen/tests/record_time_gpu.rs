//! A camera render draws the renderers that are drawable when the script calls it, on a real
//! adapter (lavapipe in CI), with predicted pixels. The scene is `gpu.rs`'s camera: a blue clear
//! and a red quad over the left half of the target. A script that switches its object on around
//! the call and off again after it, as a bake into a render texture does, still gets the quad.

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::{
    MaterialId, OffscreenWork, PropertyValue, RenderTextureId, ShaderProperties,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{ComponentHandle, MeshRenderer, ObjectHandle, World};

use super::gpu::{MESH, Rig, SIDE, shader, vector};
use super::support::{camera, render_texture, renderer_with, world};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

struct Scene {
    world: World,
    camera: ComponentHandle,
    target: RenderTextureId,
    material: MaterialId,
}

fn scene() -> Scene {
    let mut world = world();
    let target = render_texture(&mut world, [SIDE, SIDE]);
    let camera = camera(
        &mut world,
        Camera {
            background: [0.0, 0.0, 1.0, 1.0],
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let shader = shader(&mut world, "camera");
    let material = world.create_material(shader).unwrap();
    vector(&mut world, material, "_Tint", [1.0, 0.0, 0.0, 1.0]);
    Scene {
        world,
        camera,
        target,
        material,
    }
}

impl Scene {
    /// A renderer of the red quad over the left half, on an object that is active or not.
    fn quad(&mut self, active: bool) -> ObjectHandle {
        let mut block = ShaderProperties::default();
        block
            .set(PropertyValue::Vector(
                "_Place".into(),
                [0.5, 1.0, -0.5, 0.0],
            ))
            .unwrap();
        let renderer = MeshRenderer {
            material_count: 1,
            slot_materials: vec![Some(self.material)],
            property_block: Some(block),
            ..MeshRenderer::default()
        };
        renderer_with(&mut self.world, (0, active), MESH, renderer)
    }

    fn render(&mut self) {
        self.world
            .record_offscreen(OffscreenWork::CameraRender {
                camera: self.camera,
                target: self.target,
            })
            .unwrap();
    }

    /// The target's image after the frame's work runs on the adapter.
    fn image(&mut self, rig: &mut Rig) -> Vec<u8> {
        let (adapted, operations, reads) = rig.frame(&mut self.world, &[self.target]);
        assert!(
            adapted.report.failures.is_empty(),
            "{:?}",
            adapted.report.failures
        );
        rig.run(operations);
        reads.into_iter().next().unwrap().bytes(&rig.device)
    }
}

fn pixel(bytes: &[u8], (x, y): (usize, usize)) -> [u8; 4] {
    bytes[(y * SIDE as usize + x) * 4..][..4]
        .try_into()
        .unwrap()
}

/// Whether the quad covers the left half and the clear shows over the right.
fn shows_the_quad(image: &[u8]) -> bool {
    (0..SIDE as usize).all(|y| {
        pixel(image, (0, y)) == RED
            && pixel(image, (1, y)) == RED
            && pixel(image, (2, y)) == BLUE
            && pixel(image, (3, y)) == BLUE
    })
}

fn shows_only_the_clear(image: &[u8]) -> bool {
    image.chunks(4).all(|texel| texel == BLUE)
}

#[test]
fn an_object_that_is_active_only_around_the_render_is_drawn_into_the_target() {
    let mut rig = Rig::new();
    let mut scene = scene();
    let object = scene.quad(false);
    scene.world.set_active(object, true).unwrap();
    scene.render();
    scene.world.set_active(object, false).unwrap();
    let image = scene.image(&mut rig);
    assert!(shows_the_quad(&image), "{image:?}");
}

#[test]
fn an_object_that_is_deactivated_after_the_render_is_still_drawn_into_the_target() {
    let mut rig = Rig::new();
    let mut scene = scene();
    let object = scene.quad(true);
    scene.render();
    scene.world.set_active(object, false).unwrap();
    let image = scene.image(&mut rig);
    assert!(shows_the_quad(&image), "{image:?}");
}

#[test]
fn an_object_that_is_activated_after_the_render_is_not_drawn_by_it() {
    let mut rig = Rig::new();
    let mut scene = scene();
    let object = scene.quad(false);
    scene.render();
    scene.world.set_active(object, true).unwrap();
    let image = scene.image(&mut rig);
    assert!(shows_only_the_clear(&image), "{image:?}");
}

#[test]
fn an_object_that_stays_inactive_is_not_drawn() {
    let mut rig = Rig::new();
    let mut scene = scene();
    scene.quad(false);
    scene.render();
    let image = scene.image(&mut rig);
    assert!(shows_only_the_clear(&image), "{image:?}");
}
