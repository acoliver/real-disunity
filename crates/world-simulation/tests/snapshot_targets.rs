//! What a frame's snapshot holds for scriptable-pipeline work: the render textures its records
//! name, even when a script destroyed one later in the same frame, and the cameras that set up
//! or cull, with the renderers they may draw.
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, OffscreenWork, RenderTextureId, TargetRef, WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, MeshFilter, MeshRenderer, ObjectDesc, TimeSpan, World,
    WorldSettings,
};

const MESH: u64 = (1 << 41) + 9;

fn setup(camera: ComponentHandle, target: RenderTextureId) -> OffscreenWork {
    OffscreenWork::SetupCamera {
        camera,
        view: std::array::from_fn(|index| if index % 5 == 0 { 1.0 } else { 0.0 }),
        projection: std::array::from_fn(|index| if index % 5 == 0 { 1.0 } else { 0.0 }),
        target: TargetRef::Render(target),
    }
}

fn cull(camera: ComponentHandle, culling_mask: u32) -> OffscreenWork {
    OffscreenWork::Cull {
        id: 1,
        camera,
        culling_mask,
        planes: vec![[0.0, 0.0, 1.0, 10.0]],
        options: 0,
    }
}

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn snapshot(world: &mut World) -> GraphicsSnapshot {
    GraphicsSnapshot::take(world).unwrap()
}

fn render_texture(world: &mut World, [width, height]: [u32; 2]) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width,
            height,
            format: 8,
            depth_bits: 0,
            mip_count: 1,
            srgb: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn masked_camera(world: &mut World, mask: u32) -> ComponentHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let settings = Camera {
        culling_mask: mask,
        ..Camera::default()
    };
    world
        .add_component(object, ComponentDesc::Camera(settings))
        .unwrap()
}

/// An active renderer of `mesh` on `layer`, with no material slots.
fn renderer(world: &mut World, layer: u8, mesh: u64) {
    let desc = ObjectDesc {
        layer,
        ..ObjectDesc::default()
    };
    let object = world.create_object(desc).unwrap();
    let filter = MeshFilter {
        mesh: 3,
        drawn_mesh: mesh,
    };
    world
        .add_component(object, ComponentDesc::MeshFilter(filter))
        .unwrap();
    world
        .add_component(object, ComponentDesc::MeshRenderer(MeshRenderer::default()))
        .unwrap();
}

#[test]
fn a_texture_destroyed_after_a_record_named_it_stays_in_that_frames_snapshot() {
    let mut world = world();
    let camera = masked_camera(&mut world, 1);
    let target = render_texture(&mut world, [24, 16]);
    world.record_offscreen(setup(camera, target)).unwrap();
    world.destroy_render_texture(target).unwrap();
    assert!(
        world.render_texture(target).is_err(),
        "the world no longer holds the texture"
    );
    let taken = snapshot(&mut world);
    let held = taken
        .render_textures
        .get(&target)
        .expect("the snapshot keeps the description the record needs");
    assert_eq!([held.width, held.height], [24, 16]);
    let next = snapshot(&mut world);
    assert!(
        next.render_textures.is_empty(),
        "the next frame forgets the destroyed texture"
    );
}

#[test]
fn a_texture_destroyed_before_any_record_of_the_frame_is_not_kept() {
    let mut world = world();
    let camera = masked_camera(&mut world, 1);
    let gone = render_texture(&mut world, [8, 8]);
    let target = render_texture(&mut world, [8, 8]);
    world.destroy_render_texture(gone).unwrap();
    world.record_offscreen(setup(camera, target)).unwrap();
    let taken = snapshot(&mut world);
    assert!(taken.render_textures.contains_key(&target));
    assert!(
        !taken.render_textures.contains_key(&gone),
        "only work recorded before the destroy can name it"
    );
}

#[test]
fn a_texture_no_record_names_is_not_kept_though_the_frame_has_work() {
    let mut world = world();
    let camera = masked_camera(&mut world, 1);
    let unnamed = render_texture(&mut world, [8, 8]);
    let target = render_texture(&mut world, [8, 8]);
    world.record_offscreen(setup(camera, target)).unwrap();
    world.destroy_render_texture(unnamed).unwrap();
    let taken = snapshot(&mut world);
    assert!(taken.render_textures.contains_key(&target));
    assert!(
        !taken.render_textures.contains_key(&unnamed),
        "the host frees a destroyed texture the frame's work does not name"
    );
}

#[test]
fn every_kind_of_record_that_reads_or_draws_a_texture_names_it() {
    use worldrunner_world_simulation::graphics::{PropertyValue, TextureRef, TextureSlot};
    let mut world = world();
    let camera = masked_camera(&mut world, 1);
    let id = render_texture(&mut world, [8, 8]);
    let other = render_texture(&mut world, [8, 8]);
    let slot = TextureSlot {
        texture: TextureRef::Render(id),
        ..TextureSlot::default()
    };
    let named = [
        OffscreenWork::CameraRender { camera, target: id },
        OffscreenWork::Blit {
            source: TextureRef::Render(id),
            target: other,
            material: None,
            pass: 0,
        },
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::CameraTarget],
            depth: Some(TargetRef::Render(id)),
            mip: 0,
            face: -1,
            slice: 0,
        },
        OffscreenWork::SetGlobal(PropertyValue::Texture("_Main".into(), slot)),
        setup(camera, id),
    ];
    for work in &named {
        assert!(work.names_render_texture(id), "{work:?}");
        assert!(!work.names_render_texture(other) || matches!(work, OffscreenWork::Blit { .. }));
    }
    assert!(!cull(camera, 1).names_render_texture(id));
}

/// A frame that ends without a snapshot forgets its destroyed textures with its work.
#[test]
fn a_destroyed_texture_does_not_outlive_its_frame_or_its_taken_work() {
    for take_work in [false, true] {
        let mut world = world();
        let camera = masked_camera(&mut world, 1);
        let gone = render_texture(&mut world, [8, 8]);
        let target = render_texture(&mut world, [8, 8]);
        world.record_offscreen(setup(camera, gone)).unwrap();
        world.destroy_render_texture(gone).unwrap();
        if take_work {
            assert_eq!(world.take_offscreen().len(), 1);
        } else {
            world
                .advance(TimeSpan::from_nanos(16_000_000).unwrap())
                .unwrap();
        }
        world.record_offscreen(setup(camera, target)).unwrap();
        let taken = snapshot(&mut world);
        assert!(
            !taken.render_textures.contains_key(&gone),
            "the destroyed texture went with the work that named it (taken: {take_work})"
        );
    }
}

#[test]
fn new_work_cannot_name_a_destroyed_texture() {
    let mut world = world();
    let camera = masked_camera(&mut world, 1);
    let target = render_texture(&mut world, [8, 8]);
    world.record_offscreen(setup(camera, target)).unwrap();
    world.destroy_render_texture(target).unwrap();
    assert!(world.record_offscreen(setup(camera, target)).is_err());
}

#[test]
fn a_camera_that_sets_up_is_in_the_snapshot_with_the_renderers_it_may_draw() {
    let mut world = world();
    let camera = masked_camera(&mut world, 1 << 5);
    let target = render_texture(&mut world, [8, 8]);
    renderer(&mut world, 5, MESH);
    world.record_offscreen(setup(camera, target)).unwrap();
    let taken = snapshot(&mut world);
    assert!(taken.cameras.contains_key(&camera));
    assert_eq!(
        taken.renderers.len(),
        1,
        "the camera's layer admits the renderer"
    );
}

#[test]
fn a_cull_names_its_camera_and_its_own_mask_admits_renderers() {
    let mut world = world();
    let camera = masked_camera(&mut world, 1);
    renderer(&mut world, 3, MESH);
    world.record_offscreen(cull(camera, 1 << 3)).unwrap();
    let taken = snapshot(&mut world);
    assert!(taken.cameras.contains_key(&camera));
    assert_eq!(
        taken.renderers.len(),
        1,
        "the cull's mask admits layer 3, which the camera's own mask leaves out"
    );
}
