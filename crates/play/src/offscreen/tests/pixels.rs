//! Pixel reads in the synchronous GPU flush on a real adapter (lavapipe in CI): a blit writes a
//! known gradient into a render texture, a read of a rectangle of it is recorded after the blit
//! in call order, and the world gets the texels back through the sink the flush applies to.
//!
//! The gradient shader writes the texture coordinates as red and green, so the texel at column c
//! and row r (counted from the bottom, as the world counts) holds the centre of that column and
//! row of a four texel side: 0.125, 0.375, 0.625 and 0.875 in 8-bit channels. Every expected
//! float comes from those centres and the 8-bit rounding, not from the output.

use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, TextureRef, WorldRenderTexture, WorldSampler,
};

use super::super::flush::OffscreenHost;
use super::super::pixels::{REFUSED_BUDGET, REFUSED_INTEGER, REFUSED_NO_COPY};
use super::flush::{host, host_with_budget};
use super::gpu::{Quad, SIDE, shader, target, vector};
use super::support::{snapshot, world};

/// The 8-bit values of the centres of the four columns or rows: 255 * (0.125, 0.375, ...)
/// rounded to the nearest integer (31.875, 95.625, 159.375, 223.125).
const CENTRES: [u8; 4] = [32, 96, 159, 223];
/// `GraphicsFormat` 8, four 8-bit channels (the managed ARGB32), and 4, the same stored sRGB.
const ARGB32: u32 = 8;
const ARGB32_SRGB: u32 = 4;
/// `GraphicsFormat` 45, one 16-bit float channel (a height map's).
const HALF: u32 = 45;
/// `GraphicsFormat` 37, one 32-bit unsigned integer channel.
const INTEGER: u32 = 37;

fn stored(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

/// A blit of pass `pass` of the gradient shader from `source` into `target`, with `tint`.
fn blit(
    world: &mut World,
    (source, target): (RenderTextureId, RenderTextureId),
    pass: i32,
    tint: [f32; 4],
) {
    let gradient = shader(world, "gradient");
    let material = world.create_material(gradient).unwrap();
    vector(world, material, "_Tint", tint);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target,
            material: Some(material),
            pass,
        })
        .unwrap();
}

fn render_texture_of(world: &mut World, format: u32, srgb: bool) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: SIDE,
            height: SIDE,
            format,
            depth_bits: 0,
            mip_count: 1,
            srgb,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

/// The texels the gradient writes at the columns `columns` of the rows `rows`, row by row.
fn gradient(columns: std::ops::Range<usize>, rows: std::ops::Range<usize>) -> Vec<f32> {
    rows.flat_map(|row| {
        columns
            .clone()
            .flat_map(move |column| [stored(CENTRES[column]), stored(CENTRES[row]), 1.0, 1.0])
    })
    .collect()
}

fn flush(host: &mut OffscreenHost, world: &mut World) -> super::super::flush::FlushOutcome {
    let snapshot = snapshot(world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    outcome.apply(world).unwrap();
    outcome
}

#[test]
fn a_read_of_a_rectangle_answers_the_texels_a_blit_wrote_bottom_row_first() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (source, drawn) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    blit(&mut world, (source, drawn), 0, [1.0; 4]);
    // Columns 1 and 2 of rows 1, 2 and 3 of the four by four texture.
    let request = world.request_texture_pixels(drawn, 1, 1, 2, 3).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert_eq!(
        world.take_texture_pixels(request),
        Some(gradient(1..3, 1..4)),
        "row 1 first: its green is the second centre, and red climbs along each row"
    );
    assert_eq!(outcome.report.pixel_reads, 1);
    assert!(outcome.report.pixel_reads_refused.is_empty());
}

#[test]
fn a_clear_on_the_bound_target_sets_the_texels_a_later_read_answers() {
    use worldrunner_world_simulation::graphics::TargetRef;
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (drawn, other) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    for work in [
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(drawn)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value: [1.0, 0.0, 0.2, 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        },
    ] {
        world.record_offscreen(work).unwrap();
    }
    let cleared = world.request_texture_pixels(drawn, 0, 0, 4, 4).unwrap();
    let untouched = world.request_texture_pixels(other, 0, 0, 1, 1).unwrap();

    let outcome = flush(&mut host, &mut world);

    // 0.2 of 255 is exactly 51, so the stored value reads back as 51 / 255.
    assert_eq!(
        world.take_texture_pixels(cleared),
        Some([1.0, 0.0, stored(51), 1.0].repeat(16))
    );
    assert_eq!(world.take_texture_pixels(untouched), Some(vec![0.0; 4]));
    assert_eq!(outcome.report.pixel_reads, 2);
}

#[test]
fn a_read_sees_the_work_recorded_before_it_and_not_the_work_recorded_after() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (source, drawn) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    let before = world.request_texture_pixels(drawn, 0, 0, 2, 1).unwrap();
    blit(&mut world, (source, drawn), 0, [1.0; 4]);
    let after = world.request_texture_pixels(drawn, 0, 0, 2, 1).unwrap();
    blit(&mut world, (source, drawn), 1, [0.0, 1.0, 0.0, 1.0]);
    let last = world.request_texture_pixels(drawn, 0, 0, 2, 1).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert_eq!(world.take_texture_pixels(before), Some(vec![0.0; 8]));
    assert_eq!(world.take_texture_pixels(after), Some(gradient(0..2, 0..1)));
    assert_eq!(
        world.take_texture_pixels(last),
        Some([0.0, 1.0, 0.0, 1.0].repeat(2))
    );
    assert_eq!(outcome.report.pixel_reads, 3);
}

#[test]
fn an_srgb_render_texture_answers_the_values_it_stores_and_does_not_decode_them() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let source = target(&mut world, ARGB32);
    let drawn = render_texture_of(&mut world, ARGB32_SRGB, true);
    // The linear value whose sRGB encoding is 128 / 255:
    // ((128 / 255 + 0.055) / 1.055) ^ 2.4 = 0.2158605.
    let linear = ((128.0_f32 / 255.0 + 0.055) / 1.055).powf(2.4);
    blit(
        &mut world,
        (source, drawn),
        1,
        [linear, linear, linear, 1.0],
    );
    let request = world.request_texture_pixels(drawn, 0, 0, 1, 1).unwrap();

    flush(&mut host, &mut world);

    let found = world.take_texture_pixels(request).unwrap();
    assert_eq!(
        found,
        [stored(128), stored(128), stored(128), 1.0],
        "the byte the target stores over 255"
    );
    assert!(
        (found[0] - linear).abs() > 0.2,
        "not the linear value {linear}"
    );
}

#[test]
fn a_half_float_render_texture_answers_its_stored_floats_with_zero_green_and_blue_and_one_alpha() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (source, heights) = (target(&mut world, ARGB32), target(&mut world, HALF));
    blit(&mut world, (source, heights), 0, [1.0; 4]);
    let request = world.request_texture_pixels(heights, 0, 2, 4, 1).unwrap();

    flush(&mut host, &mut world);

    // Red is the column's centre, 0.125 to 0.875, which a half float stores exactly.
    let row: Vec<f32> = [0.125, 0.375, 0.625, 0.875]
        .into_iter()
        .flat_map(|red| [red, 0.0, 0.0, 1.0])
        .collect();
    assert_eq!(world.take_texture_pixels(request), Some(row));
}

#[test]
fn a_read_of_an_integer_render_texture_is_refused_and_counted_and_the_others_still_answer() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (source, drawn) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    let integers = render_texture_of(&mut world, INTEGER, false);
    let refused = world.request_texture_pixels(integers, 0, 0, 2, 2).unwrap();
    blit(&mut world, (source, drawn), 0, [1.0; 4]);
    let answered = world.request_texture_pixels(drawn, 3, 3, 1, 1).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert_eq!(
        world.take_texture_pixels(refused),
        None,
        "no answer was written"
    );
    assert_eq!(
        world.take_texture_pixels(answered),
        Some(gradient(3..4, 3..4))
    );
    assert_eq!(outcome.report.pixel_reads, 1);
    assert_eq!(
        outcome
            .report
            .pixel_reads_refused
            .iter()
            .map(|(why, count)| (why.as_str(), *count))
            .collect::<Vec<_>>(),
        [(REFUSED_INTEGER, 1)]
    );
}

#[test]
fn a_read_of_a_texture_nothing_drew_into_answers_zeros() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let idle = target(&mut world, ARGB32);
    let request = world.request_texture_pixels(idle, 0, 0, 4, 4).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert_eq!(world.take_texture_pixels(request), Some(vec![0.0; 64]));
    assert_eq!(outcome.report.pixel_reads, 1);
}

/// A world that blits into a render texture and reads one texel of it when `read` is set.
fn blit_world(read: bool) -> (World, Option<u64>) {
    let mut world = world();
    let (source, drawn) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    blit(&mut world, (source, drawn), 0, [1.0; 4]);
    let request = read.then(|| world.request_texture_pixels(drawn, 0, 0, 1, 1).unwrap());
    (world, request)
}

#[test]
fn a_read_the_budget_has_no_room_to_copy_is_refused_and_counted_with_its_cause() {
    let directory = tempfile::tempdir().unwrap();
    // The storage the blit alone holds once the GPU is done with it.
    let mut measured = host(directory.path());
    let (mut world, _) = blit_world(false);
    flush(&mut measured, &mut world);
    measured.settle();
    let held = measured.live_bytes();

    // A budget of exactly that has nothing left for the copy a read stages.
    let mut host = host_with_budget(directory.path(), held);
    let (mut world, request) = blit_world(true);
    let outcome = flush(&mut host, &mut world);

    assert_eq!(world.take_texture_pixels(request.unwrap()), None);
    assert_eq!(outcome.report.pixel_reads, 0);
    assert_eq!(
        outcome
            .report
            .pixel_reads_refused
            .iter()
            .map(|(why, count)| (why.as_str(), *count))
            .collect::<Vec<_>>(),
        [(format!("{REFUSED_NO_COPY}: {REFUSED_BUDGET}").as_str(), 1)]
    );
}
