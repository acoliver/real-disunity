//! A global texture that an executed command buffer set stays set for the takes after it, on a
//! real adapter (lavapipe in CI).
//!
//! The managed engine runs a command buffer's global writes through the same path as
//! `Shader.SetGlobalTexture`: a direct write to the world's one global state, which the host's
//! next take starts from. The first take fills a render texture and sets `_Last` to it; the
//! second take draws two quads that sample `_Last`. The host keeps the texture's pixels between
//! takes, so the second take must read the fill and no stand-in. A global that only an ordered
//! record set ends with the take that carried the record, which the last test pins as the reason
//! the managed side writes the state directly.

use super::super::flush::OffscreenHost;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, RenderTextureId, TextureRef,
};

use super::flush::host;
use super::gpu::{Quad, SIDE, target};
use super::lights_gpu::{filled, quad, record, two_lists};
use super::ordered_globals_gpu::{FILL, GREY, LAST, fill, set_last};
use super::support::{assert_close, snapshot, texture_slot, world};

const ARGB32: u32 = 8;

/// Flushes the world's current take on `host`, and returns the pixels of `drawn` after it with
/// the number of draws the host made with a stand-in for `_Last`.
fn take(host: &mut OffscreenHost, world: &mut World, drawn: RenderTextureId) -> (Vec<f32>, usize) {
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let snapshot = snapshot(world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(world).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    let undeclared = format!("{LAST} undeclared");
    let stand_ins = outcome
        .report
        .stand_ins
        .by_property
        .get(&undeclared)
        .copied();
    (
        world.take_texture_pixels(request).unwrap(),
        stand_ins.unwrap_or(0),
    )
}

/// A world with two quads that sample `_Last`, over the left and the right half of `drawn`, and
/// a render texture `source` that the first take of the test fills.
fn sampling_world() -> (World, RenderTextureId, RenderTextureId) {
    let mut world = world();
    let (drawn, source) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    quad(&mut world, "global_sampler", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "global_sampler", 1, [0.5, 1.0, 0.5, 0.0]);
    (world, drawn, source)
}

/// Draws both quads in a take of their own, with nothing set inside it.
fn draw_take(world: &mut World, drawn: RenderTextureId) {
    two_lists(world, drawn, (&|_| {}, &|_| {}));
}

#[test]
fn a_global_texture_a_buffer_set_in_one_take_is_read_by_the_draws_of_the_next() {
    let (mut world, drawn, source) = sampling_world();
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    // First take: the buffer fills the texture and sets the global with the write
    // `Shader.SetGlobalTexture` makes; nothing in this take reads it.
    fill(&mut world, source);
    set_last(&mut world, TextureRef::Render(source));
    take(&mut host, &mut world, drawn);
    // Second take: the world's state holds the texture, so the draws read its pixels.
    draw_take(&mut world, drawn);
    let (read, stand_ins) = take(&mut host, &mut world, drawn);
    assert_close(&read, &filled(FILL));
    assert_eq!(stand_ins, 0);
}

#[test]
fn a_global_texture_only_an_ordered_record_set_ends_with_its_take() {
    let (mut world, drawn, source) = sampling_world();
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    fill(&mut world, source);
    record(
        &mut world,
        OffscreenWork::SetGlobal(PropertyValue::Texture(
            LAST.into(),
            texture_slot(TextureRef::Render(source)),
        )),
    );
    take(&mut host, &mut world, drawn);
    draw_take(&mut world, drawn);
    let (read, stand_ins) = take(&mut host, &mut world, drawn);
    // Every draw of the second take reads the grey stand-in: the camera render that opens the
    // pipeline draws both quads and each of the two lists draws one.
    assert_close(&read, &super::lights_gpu::halves(GREY, GREY));
    assert_eq!(stand_ins, 4);
}
