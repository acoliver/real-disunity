//! A command buffer's scissor rectangle and global depth bias: recorded in call order with the
//! frame's other work, refused when a value is not finite or a size is negative, and carried by
//! the wire.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode_work, encode_work};
use worldrunner_world_simulation::graphics::{GraphicsError, OffscreenWork};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn scissor_and_bias_records_keep_their_place_in_the_frame() {
    let mut world = world();
    let records = vec![
        OffscreenWork::EnableScissor {
            rect: [4.0, 4.0, 56.0, 56.0],
        },
        OffscreenWork::SetDepthBias {
            bias: 1.0,
            slope_bias: 2.5,
        },
        OffscreenWork::DisableScissor,
        OffscreenWork::SetDepthBias {
            bias: 0.0,
            slope_bias: 0.0,
        },
    ];
    for record in records.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.take_offscreen().0, records);
    assert!(world.take_offscreen().0.is_empty());
}

#[test]
fn a_scissor_with_a_component_that_is_not_finite_is_refused() {
    let mut world = world();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for index in 0..4 {
            let mut rect = [1.0, 2.0, 3.0, 4.0];
            rect[index] = bad;
            assert_eq!(
                graphics_error(world.record_offscreen(OffscreenWork::EnableScissor { rect })),
                GraphicsError::NotFinite
            );
        }
    }
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_scissor_with_a_negative_size_is_refused_and_a_negative_origin_is_recorded() {
    let mut world = world();
    for rect in [[0.0, 0.0, -1.0, 4.0], [0.0, 0.0, 4.0, -0.5]] {
        assert_eq!(
            graphics_error(world.record_offscreen(OffscreenWork::EnableScissor { rect })),
            GraphicsError::InvalidOffscreen
        );
    }
    world
        .record_offscreen(OffscreenWork::EnableScissor {
            rect: [-8.0, -8.0, 0.0, 0.0],
        })
        .unwrap();
    assert_eq!(world.offscreen_work().len(), 1);
}

#[test]
fn a_depth_bias_that_is_not_finite_is_refused() {
    let mut world = world();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for (bias, slope_bias) in [(bad, 1.0), (1.0, bad)] {
            assert_eq!(
                graphics_error(
                    world.record_offscreen(OffscreenWork::SetDepthBias { bias, slope_bias })
                ),
                GraphicsError::NotFinite
            );
        }
    }
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn the_wire_carries_the_scissor_and_the_bias_under_tags_twenty_to_twenty_two() {
    let enable = OffscreenWork::EnableScissor {
        rect: [-1.5, 2.0, 640.0, 0.25],
    };
    let bytes = encode_work(&enable).unwrap();
    assert_eq!(bytes[0], 20);
    assert_eq!(bytes.len(), 1 + 4 * 4);
    assert_eq!(decode_work(&bytes).unwrap(), enable);
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());

    let bytes = encode_work(&OffscreenWork::DisableScissor).unwrap();
    assert_eq!(bytes, vec![21]);
    assert_eq!(decode_work(&bytes).unwrap(), OffscreenWork::DisableScissor);

    let bias = OffscreenWork::SetDepthBias {
        bias: -1.0,
        slope_bias: 2.5,
    };
    let bytes = encode_work(&bias).unwrap();
    assert_eq!(bytes[0], 22);
    assert_eq!(bytes.len(), 1 + 2 * 4);
    assert_eq!(decode_work(&bytes).unwrap(), bias);
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());
}
