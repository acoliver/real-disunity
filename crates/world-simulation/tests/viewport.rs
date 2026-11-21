//! A command buffer's viewport: recorded in call order with the frame's other work, refused when a
//! component is not finite or a size is negative, and carried by the wire.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode_work, encode_work};
use worldrunner_world_simulation::graphics::{
    GraphicsError, OffscreenWork, RenderTextureId, WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn viewport(rect: [f32; 4]) -> OffscreenWork {
    OffscreenWork::SetViewport { rect }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn viewports_are_recorded_in_call_order_and_taken_with_the_frame() {
    let mut world = world();
    let records = vec![
        viewport([0.0, 0.0, 64.0, 32.0]),
        OffscreenWork::SetGlobalKeyword {
            name: "_FOG".into(),
            enabled: true,
        },
        viewport([-4.0, 2.5, 0.0, 0.0]),
    ];
    for record in records.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.take_offscreen().0, records);
    assert!(world.take_offscreen().0.is_empty());
}

#[test]
fn a_component_that_is_not_finite_is_refused_wherever_it_sits() {
    let mut world = world();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for index in 0..4 {
            let mut rect = [1.0, 2.0, 3.0, 4.0];
            rect[index] = bad;
            assert_eq!(
                graphics_error(world.record_offscreen(viewport(rect))),
                GraphicsError::NotFinite
            );
        }
    }
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_negative_width_or_height_is_refused() {
    let mut world = world();
    for rect in [[0.0, 0.0, -1.0, 4.0], [0.0, 0.0, 4.0, -0.5]] {
        assert_eq!(
            graphics_error(world.record_offscreen(viewport(rect))),
            GraphicsError::InvalidOffscreen
        );
    }
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_negative_origin_and_a_zero_size_are_recorded() {
    let mut world = world();
    world
        .record_offscreen(viewport([-8.0, -8.0, 0.0, 0.0]))
        .unwrap();
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn the_wire_carries_the_rectangle_under_tag_eighteen() {
    let record = viewport([-1.5, 2.0, 640.0, 0.25]);
    let bytes = encode_work(&record).unwrap();
    assert_eq!(bytes[0], 18);
    assert_eq!(bytes.len(), 1 + 4 * 4);
    assert_eq!(decode_work(&bytes).unwrap(), record);
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn a_viewport_names_no_render_texture() {
    let mut world = world();
    let texture: RenderTextureId = world
        .create_render_texture(WorldRenderTexture {
            width: 4,
            height: 4,
            format: 8,
            depth_bits: 0,
            mip_count: 1,
            srgb: false,
            dimension: 2,
            volume_depth: 1,
            auto_generate_mips: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap();
    assert!(!viewport([0.0, 0.0, 1.0, 1.0]).names_render_texture(texture));
}
