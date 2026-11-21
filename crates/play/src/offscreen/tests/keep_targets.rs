//! A render texture the world still holds keeps its storage and contents from one flush to the
//! next, whatever the work of a given flush. The player's flush handler takes a snapshot each time
//! the managed code asks for one, and some of those snapshots have no records.
//!
//! The contents are a clear colour whose 8-bit values are exact, read back through a copy and
//! the world's pixel reads, as the terrain read-back does.

use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, TargetRef, TextureRef,
};

use super::super::flush::OffscreenHost;
use super::flush::host;
use super::gpu::{Quad, target};
use super::support::{snapshot, world};

/// `GraphicsFormat` 8, four 8-bit channels.
const ARGB32: u32 = 8;
/// The colour the texture is cleared to; 0.2 of 255 is exactly 51.
const CLEARED: [f32; 4] = [1.0, 0.0, 51.0 / 255.0, 1.0];

fn flush(host: &mut OffscreenHost, world: &mut World) {
    let snapshot = snapshot(world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    outcome.apply(world).unwrap();
}

fn clear(world: &mut World, drawn: RenderTextureId) {
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
}

/// What a copy of `source` holds when the flush reads it, over the whole four by four texture.
fn copied(
    host: &mut OffscreenHost,
    world: &mut World,
    (source, copy): (RenderTextureId, RenderTextureId),
) -> Vec<f32> {
    world
        .record_offscreen(OffscreenWork::CopyTexture {
            source: TextureRef::Render(source),
            source_element: 0,
            source_mip: 0,
            target: TargetRef::Render(copy),
            target_element: 0,
            target_mip: 0,
        })
        .unwrap();
    let request = world.request_texture_pixels(copy, 0, 0, 4, 4).unwrap();
    flush(host, world);
    world.take_texture_pixels(request).unwrap()
}

#[test]
fn a_render_texture_keeps_its_contents_through_a_flush_with_no_work() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (drawn, copy) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    clear(&mut world, drawn);
    flush(&mut host, &mut world);

    flush(&mut host, &mut world);

    assert_eq!(
        copied(&mut host, &mut world, (drawn, copy)),
        CLEARED.repeat(16),
        "the texture still holds what the first flush wrote"
    );
}

#[test]
fn a_flush_with_no_work_keeps_the_storage_of_the_render_textures_the_world_holds() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    clear(&mut world, drawn);
    flush(&mut host, &mut world);
    let held = host.live_bytes();

    flush(&mut host, &mut world);

    assert_eq!(host.live_bytes(), held, "the idle flush gave nothing back");
}

#[test]
fn a_render_texture_keeps_its_contents_through_a_flush_that_does_not_name_it() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (drawn, other, copy) = (
        target(&mut world, ARGB32),
        target(&mut world, ARGB32),
        target(&mut world, ARGB32),
    );
    clear(&mut world, drawn);
    flush(&mut host, &mut world);

    clear(&mut world, other);
    flush(&mut host, &mut world);

    assert_eq!(
        copied(&mut host, &mut world, (drawn, copy)),
        CLEARED.repeat(16)
    );
}

#[test]
fn a_render_texture_the_world_destroyed_is_released_by_a_flush_with_no_work() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    clear(&mut world, drawn);
    flush(&mut host, &mut world);
    let held = host.live_bytes();

    world.destroy_render_texture(drawn).unwrap();
    flush(&mut host, &mut world);

    assert!(
        host.live_bytes() < held,
        "the destroyed texture's storage went back: {} of {held}",
        host.live_bytes()
    );
}

#[test]
fn a_render_texture_the_world_destroyed_is_released_by_a_flush_with_work() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (drawn, other) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    clear(&mut world, drawn);
    clear(&mut world, other);
    flush(&mut host, &mut world);
    let held = host.live_bytes();

    world.destroy_render_texture(drawn).unwrap();
    clear(&mut world, other);
    flush(&mut host, &mut world);

    assert!(
        host.live_bytes() < held,
        "the destroyed texture's storage went back: {} of {held}",
        host.live_bytes()
    );
}
