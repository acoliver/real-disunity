//! A command buffer's view and projection change: recorded in call order with the frame's other
//! work, refused when a matrix holds a value that is not finite, and carried by the wire.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode_work, encode_work};
use worldrunner_world_simulation::graphics::{
    GraphicsError, OffscreenWork, RenderTextureId, WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn matrix(seed: f32) -> [f32; 16] {
    std::array::from_fn(|index| seed + 0.5 * f32::from(u8::try_from(index).unwrap()))
}

fn view_projection(view: [f32; 16], projection: [f32; 16]) -> OffscreenWork {
    OffscreenWork::SetViewProjection { view, projection }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn view_changes_are_recorded_in_call_order_and_taken_with_the_frame() {
    let mut world = world();
    let records = vec![
        view_projection(matrix(1.0), matrix(-2.0)),
        OffscreenWork::SetGlobalKeyword {
            name: "_FOG".into(),
            enabled: true,
        },
        view_projection(matrix(3.0), matrix(0.25)),
    ];
    for record in records.clone() {
        world.record_offscreen(record).unwrap();
    }
    assert_eq!(world.take_offscreen(), records);
    assert!(world.take_offscreen().is_empty());
}

#[test]
fn a_matrix_value_that_is_not_finite_is_refused_in_either_matrix() {
    let mut world = world();
    for (index, bad) in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY]
        .into_iter()
        .enumerate()
    {
        let mut view = matrix(1.0);
        view[index * 5] = bad;
        assert_eq!(
            graphics_error(world.record_offscreen(view_projection(view, matrix(1.0)))),
            GraphicsError::NotFinite
        );
        let mut projection = matrix(1.0);
        projection[15 - index] = bad;
        assert_eq!(
            graphics_error(world.record_offscreen(view_projection(matrix(1.0), projection))),
            GraphicsError::NotFinite
        );
    }
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn the_wire_carries_both_matrices_under_tag_fifteen() {
    let record = view_projection(matrix(-1.5), matrix(7.0));
    let bytes = encode_work(&record).unwrap();
    assert_eq!(bytes[0], 15);
    assert_eq!(bytes.len(), 1 + 2 * 16 * 4);
    assert_eq!(decode_work(&bytes).unwrap(), record);
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn a_view_change_names_no_render_texture() {
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
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap();
    let record = view_projection(matrix(0.0), matrix(0.0));
    assert!(!record.names_render_texture(texture));
}
