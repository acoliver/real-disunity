//! The offscreen work a frame records: validation, call order and draining.
use worldrunner_world_simulation::graphics::{
    GraphicsError, MaterialId, OffscreenWork, RenderTextureId, TextureRef, WorldRenderTexture,
    WorldSampler, WorldShader,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle, ObjectDesc, TimeSpan, World, WorldError, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn texture(mip_count: u32) -> WorldRenderTexture {
    WorldRenderTexture {
        width: 16,
        height: 16,
        format: 8,
        depth_bits: 0,
        mip_count,
        srgb: false,
        dimension: 2,
        volume_depth: 1,
        sampler: WorldSampler::default(),
        generation: 0,
    }
}

fn camera(world: &mut World) -> ComponentHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap()
}

fn material(world: &mut World) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Hidden/Blit".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn blit(source: TextureRef, target: RenderTextureId) -> OffscreenWork {
    OffscreenWork::Blit {
        source,
        target,
        material: None,
        pass: -1,
    }
}

#[test]
fn records_come_back_in_call_order_and_then_the_list_is_empty() {
    let mut world = world();
    let camera = camera(&mut world);
    let a = world.create_render_texture(texture(1)).unwrap();
    let b = world.create_render_texture(texture(1)).unwrap();
    let material = material(&mut world);
    let work = [
        OffscreenWork::CameraRender { camera, target: a },
        OffscreenWork::Blit {
            source: TextureRef::Render(a),
            target: b,
            material: Some(material),
            pass: 2,
        },
        OffscreenWork::CopyTexture {
            source: TextureRef::Render(b),
            source_element: 0,
            source_mip: 0,
            target: a,
            target_element: 0,
            target_mip: 0,
        },
        blit(TextureRef::Asset(1 << 40), a),
    ];
    for record in work.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.take_offscreen(), work);
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn a_camera_render_needs_a_live_camera_component() {
    let mut world = world();
    let target = world.create_render_texture(texture(1)).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let not_camera = world
        .add_component(object, ComponentDesc::AudioListener { enabled: true })
        .unwrap();
    let rejected = world.record_offscreen(OffscreenWork::CameraRender {
        camera: not_camera,
        target,
    });
    assert_eq!(graphics_error(rejected), GraphicsError::NotCamera);

    let camera = camera(&mut world);
    world.remove_component(camera).unwrap();
    let stale = world.record_offscreen(OffscreenWork::CameraRender { camera, target });
    assert!(matches!(stale, Err(WorldError::StaleComponent(_))));
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn work_that_names_a_destroyed_render_texture_or_material_is_refused() {
    let mut world = world();
    let camera = camera(&mut world);
    let live = world.create_render_texture(texture(1)).unwrap();
    let dead = world.create_render_texture(texture(1)).unwrap();
    world.destroy_render_texture(dead).unwrap();
    let material = material(&mut world);
    world.destroy_material(material).unwrap();

    let refused = [
        (
            OffscreenWork::CameraRender {
                camera,
                target: dead,
            },
            GraphicsError::StaleRenderTexture,
        ),
        (
            blit(TextureRef::None, dead),
            GraphicsError::StaleRenderTexture,
        ),
        (
            blit(TextureRef::Render(dead), live),
            GraphicsError::StaleRenderTexture,
        ),
        (
            OffscreenWork::Blit {
                source: TextureRef::None,
                target: live,
                material: Some(material),
                pass: 0,
            },
            GraphicsError::StaleMaterial,
        ),
    ];
    for (record, error) in refused {
        assert_eq!(graphics_error(world.record_offscreen(record)), error);
    }
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn a_blit_pass_is_minus_one_for_all_passes_or_an_index() {
    let mut world = world();
    let target = world.create_render_texture(texture(1)).unwrap();
    for pass in [-1, 0, 7] {
        let record = OffscreenWork::Blit {
            source: TextureRef::None,
            target,
            material: None,
            pass,
        };
        world.record_offscreen(record).unwrap();
    }
    let rejected = world.record_offscreen(OffscreenWork::Blit {
        source: TextureRef::None,
        target,
        material: None,
        pass: -2,
    });
    assert_eq!(graphics_error(rejected), GraphicsError::InvalidOffscreen);
    assert_eq!(world.take_offscreen().len(), 3);
}

fn copy(
    source: TextureRef,
    source_mip: u32,
    target: RenderTextureId,
    target_mip: u32,
) -> OffscreenWork {
    OffscreenWork::CopyTexture {
        source,
        source_element: 0,
        source_mip,
        target,
        target_element: 0,
        target_mip,
    }
}

#[test]
fn a_copy_addresses_mip_levels_the_render_textures_have() {
    let mut world = world();
    let three = world.create_render_texture(texture(3)).unwrap();
    let one = world.create_render_texture(texture(1)).unwrap();
    world
        .record_offscreen(copy(TextureRef::Render(three), 2, three, 1))
        .unwrap();
    world
        .record_offscreen(copy(TextureRef::Asset(1 << 40), 5, three, 2))
        .unwrap();
    for refused in [
        copy(TextureRef::Render(three), 0, one, 1),
        copy(TextureRef::Render(one), 1, three, 0),
        copy(TextureRef::Render(three), 3, three, 0),
    ] {
        assert_eq!(
            graphics_error(world.record_offscreen(refused)),
            GraphicsError::InvalidOffscreen
        );
    }
    assert_eq!(world.take_offscreen().len(), 2);
}

#[test]
fn work_nobody_took_is_dropped_when_the_next_frame_starts() {
    let mut world = world();
    let target = world.create_render_texture(texture(1)).unwrap();
    world
        .record_offscreen(blit(TextureRef::None, target))
        .unwrap();
    world
        .advance(TimeSpan::from_nanos(16_000_000).unwrap())
        .unwrap();
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn work_recorded_during_a_frame_survives_until_it_is_taken() {
    let mut world = world();
    let target = world.create_render_texture(texture(1)).unwrap();
    world
        .advance(TimeSpan::from_nanos(16_000_000).unwrap())
        .unwrap();
    world
        .record_offscreen(blit(TextureRef::None, target))
        .unwrap();
    assert_eq!(world.take_offscreen().len(), 1);
}

#[test]
fn one_frame_holds_at_most_65536_records() {
    let mut world = world();
    let target = world.create_render_texture(texture(1)).unwrap();
    for _ in 0..65536 {
        world
            .record_offscreen(blit(TextureRef::None, target))
            .unwrap();
    }
    let full = world.record_offscreen(blit(TextureRef::None, target));
    assert!(matches!(full, Err(WorldError::CapacityExceeded(_))));
    assert_eq!(world.take_offscreen().len(), 65536);
}
