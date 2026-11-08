//! Reading pixels back from a render texture: the request the world records, its validation and
//! the store that carries the host's answer to the script.
use worldrunner_world_simulation::graphics::snapshot_wire::{
    WireError, decode, decode_work, encode, encode_work,
};
use worldrunner_world_simulation::graphics::{
    GraphicsError, GraphicsSnapshot, MAX_PENDING_READBACKS, OffscreenWork, RenderTextureId,
    TextureRef, WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::{LimitKind, TimeSpan, World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn render_texture(world: &mut World, width: u32, height: u32) -> RenderTextureId {
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

fn graphics_error<T: std::fmt::Debug>(result: Result<T, WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn floats(count: usize) -> Vec<f32> {
    (0..count)
        .map(|index| f32::from(u16::try_from(index).unwrap()) * 0.5)
        .collect()
}

#[test]
fn a_request_is_recorded_after_the_blit_before_it_and_before_the_work_after_it() {
    let mut world = world();
    let a = render_texture(&mut world, 8, 4);
    let b = render_texture(&mut world, 8, 4);
    let blit = |target| OffscreenWork::Blit {
        source: TextureRef::None,
        target,
        material: None,
        pass: -1,
    };
    world.record_offscreen(blit(a)).unwrap();
    let request = world.request_texture_pixels(a, 1, 2, 3, 1).unwrap();
    world.record_offscreen(blit(b)).unwrap();
    let work = world.offscreen_work();
    assert_eq!(work.len(), 3);
    assert_eq!(work[0], blit(a));
    assert_eq!(
        work[1],
        OffscreenWork::ReadPixels {
            request,
            texture: a,
            x: 1,
            y: 2,
            width: 3,
            height: 1,
        }
    );
    assert_eq!(work[2], blit(b));
}

#[test]
fn requests_get_increasing_nonzero_ids_that_outlive_the_frame() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let first = world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
    let second = world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
    assert!(first > 0);
    assert!(second > first);
    world
        .advance(TimeSpan::from_nanos(16_000_000).unwrap())
        .unwrap();
    let third = world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
    assert!(third > second);
}

#[test]
fn a_rect_that_fits_exactly_is_accepted() {
    let mut world = world();
    let texture = render_texture(&mut world, 8, 4);
    world.request_texture_pixels(texture, 0, 0, 8, 4).unwrap();
    world.request_texture_pixels(texture, 7, 3, 1, 1).unwrap();
    assert_eq!(world.offscreen_work().len(), 2);
}

#[test]
fn a_texture_that_is_gone_is_stale() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    world.destroy_render_texture(texture).unwrap();
    let error = graphics_error(world.request_texture_pixels(texture, 0, 0, 1, 1));
    assert_eq!(error, GraphicsError::StaleRenderTexture);
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn an_empty_or_outside_rect_is_refused_and_records_nothing() {
    let mut world = world();
    let texture = render_texture(&mut world, 8, 4);
    let rects = [
        (0, 0, 0, 1),
        (0, 0, 1, 0),
        (0, 0, 9, 1),
        (0, 0, 1, 5),
        (8, 0, 1, 1),
        (0, 4, 1, 1),
        (5, 0, 4, 1),
        (0, 2, 1, 3),
        (u32::MAX, 0, 2, 1),
        (0, u32::MAX, 1, 2),
        (1, 1, u32::MAX, 1),
        (1, 1, 1, u32::MAX),
    ];
    for (x, y, width, height) in rects {
        let result = world.request_texture_pixels(texture, x, y, width, height);
        assert_eq!(
            graphics_error(result),
            GraphicsError::ReadbackRect,
            "{x} {y} {width} {height}"
        );
    }
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_read_of_more_than_the_texel_limit_is_refused() {
    let mut world = world();
    let texture = render_texture(&mut world, 16384, 16384);
    let result = world.request_texture_pixels(texture, 0, 0, 16384, 16384);
    assert_eq!(graphics_error(result), GraphicsError::ReadbackRect);
    world
        .request_texture_pixels(texture, 0, 0, 8192, 8192)
        .unwrap();
}

#[test]
fn the_script_side_record_call_refuses_a_read_request() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let work = OffscreenWork::ReadPixels {
        request: 1,
        texture,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    assert_eq!(
        graphics_error(world.record_offscreen(work)),
        GraphicsError::InvalidOffscreen
    );
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn the_host_answer_reaches_the_script_once() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let request = world.request_texture_pixels(texture, 0, 0, 2, 3).unwrap();
    let pixels = floats(2 * 3 * 4);
    world.write_texture_pixels(request, &pixels).unwrap();
    assert_eq!(world.take_texture_pixels(request), Some(pixels));
    assert_eq!(world.take_texture_pixels(request), None);
}

#[test]
fn a_request_nobody_answered_takes_as_none_and_is_forgotten() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let request = world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
    assert_eq!(world.take_texture_pixels(request), None);
    let error = graphics_error(world.write_texture_pixels(request, &floats(4)));
    assert_eq!(error, GraphicsError::UnknownReadback);
}

#[test]
fn a_write_for_an_unknown_request_is_refused() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let request = world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
    for unknown in [0, request + 1, u64::MAX] {
        let error = graphics_error(world.write_texture_pixels(unknown, &floats(4)));
        assert_eq!(error, GraphicsError::UnknownReadback);
    }
}

#[test]
fn a_write_of_the_wrong_length_is_refused_and_keeps_the_request_open() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let request = world.request_texture_pixels(texture, 0, 0, 2, 2).unwrap();
    for count in [0, 15, 17, 32] {
        let error = graphics_error(world.write_texture_pixels(request, &floats(count)));
        assert_eq!(error, GraphicsError::ReadbackLength, "{count}");
    }
    assert_eq!(world.take_texture_pixels(request), None);
    let request = world.request_texture_pixels(texture, 0, 0, 2, 2).unwrap();
    world.write_texture_pixels(request, &floats(16)).unwrap();
    assert_eq!(world.take_texture_pixels(request), Some(floats(16)));
}

#[test]
fn answers_to_two_requests_stay_apart() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let one = world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
    let two = world.request_texture_pixels(texture, 0, 0, 2, 1).unwrap();
    world.write_texture_pixels(two, &floats(8)).unwrap();
    world
        .write_texture_pixels(one, &[9.0, 8.0, 7.0, 6.0])
        .unwrap();
    assert_eq!(
        world.take_texture_pixels(one),
        Some(vec![9.0, 8.0, 7.0, 6.0])
    );
    assert_eq!(world.take_texture_pixels(two), Some(floats(8)));
}

#[test]
fn a_new_frame_forgets_requests_nobody_took() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let request = world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
    world.write_texture_pixels(request, &floats(4)).unwrap();
    world
        .advance(TimeSpan::from_nanos(16_000_000).unwrap())
        .unwrap();
    assert_eq!(world.take_texture_pixels(request), None);
}

#[test]
fn a_world_holds_a_bounded_number_of_open_requests() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let mut first = 0;
    for index in 0..MAX_PENDING_READBACKS {
        let request = world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
        if index == 0 {
            first = request;
        }
    }
    let result = world.request_texture_pixels(texture, 0, 0, 1, 1);
    assert!(matches!(
        result,
        Err(WorldError::CapacityExceeded(LimitKind::Graphics))
    ));
    assert_eq!(world.offscreen_work().len(), MAX_PENDING_READBACKS);
    assert_eq!(world.take_texture_pixels(first), None);
    world.request_texture_pixels(texture, 0, 0, 1, 1).unwrap();
}

#[test]
fn a_read_request_round_trips_alone_and_inside_a_snapshot() {
    let mut world = world();
    let texture = render_texture(&mut world, 64, 32);
    let request = world.request_texture_pixels(texture, 3, 5, 7, 11).unwrap();
    let other = world.request_texture_pixels(texture, 0, 0, 64, 32).unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(snapshot.work.len(), 2);
    let record = OffscreenWork::ReadPixels {
        request,
        texture,
        x: 3,
        y: 5,
        width: 7,
        height: 11,
    };
    assert_eq!(snapshot.work[0], record);
    assert!(matches!(
        snapshot.work[1],
        OffscreenWork::ReadPixels { request: id, .. } if id == other
    ));
    assert_eq!(decode(&encode(&snapshot).unwrap()).unwrap(), snapshot);

    let bytes = encode_work(&record).unwrap();
    assert_eq!(decode_work(&bytes).unwrap(), record);
    assert_eq!(
        decode_work(&bytes[..bytes.len() - 1]),
        Err(WireError::Truncated)
    );
    let mut longer = bytes;
    longer.push(0);
    assert_eq!(decode_work(&longer), Err(WireError::TrailingBytes));
}

#[test]
fn every_field_of_a_read_request_survives_the_wire() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let record = OffscreenWork::ReadPixels {
        request: u64::MAX - 1,
        texture,
        x: 0x0102_0304,
        y: 0x0506_0708,
        width: 0x090a_0b0c,
        height: 0x0d0e_0f10,
    };
    assert_eq!(decode_work(&encode_work(&record).unwrap()).unwrap(), record);
}

#[test]
fn the_size_of_an_answer_is_known_before_it_is_taken() {
    let mut world = world();
    let texture = render_texture(&mut world, 4, 4);
    let request = world.request_texture_pixels(texture, 0, 0, 2, 1).unwrap();
    assert_eq!(world.answered_pixel_floats(request), None);
    world.write_texture_pixels(request, &floats(8)).unwrap();
    assert_eq!(world.answered_pixel_floats(request), Some(8));
    assert_eq!(world.answered_pixel_floats(request + 1), None);
    world.take_texture_pixels(request).unwrap();
    assert_eq!(world.answered_pixel_floats(request), None);
}
