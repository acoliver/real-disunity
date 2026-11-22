//! What the host answers when the UI reads a render texture: the device texture, and whether a
//! frame the state built wrote it. The frames are synthetic; every expectation comes from which
//! texture a record draws into.

use worldrunner_render_plan::TextureSampler;
use worldrunner_render_scene::simulation::{UiTarget, UiTargetRefusal};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, RenderTextureId, TargetRef, TextureRef, WorldRenderTexture, WorldSampler,
    dimension,
};

use super::super::flush::OffscreenHost;
use super::flush::host;
use super::gpu::{Quad, SIDE, shader, target};
use super::support::{material, snapshot, world};

/// `GraphicsFormat` 8, four 8-bit channels.
const ARGB32: u32 = 8;

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
            value: [0.0, 1.0, 0.0, 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        },
    ] {
        world.record_offscreen(work).unwrap();
    }
}

/// A blit that reads `source` and draws into `into`, so `source` is named and not written.
fn blit_from(world: &mut World, source: RenderTextureId, into: RenderTextureId) {
    let blit = shader(world, "blit");
    let material = material(world, blit);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(source),
            target: TargetRef::Render(into),
            material: Some(material),
            pass: 0,
        })
        .unwrap();
}

#[test]
fn a_texture_a_built_frame_wrote_is_offered_written() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    clear(&mut world, drawn);
    flush(&mut host, &mut world);

    let read = host.render_texture_for_ui(drawn);

    assert!(read.texture.is_some());
    assert!(read.written);
}

#[test]
fn a_texture_nothing_wrote_is_offered_unwritten() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (read_only, into, unnamed) = (
        target(&mut world, ARGB32),
        target(&mut world, ARGB32),
        target(&mut world, ARGB32),
    );
    blit_from(&mut world, read_only, into);
    flush(&mut host, &mut world);

    let named = host.render_texture_for_ui(read_only);
    assert!(
        named.texture.is_some(),
        "a frame named it, so it has storage"
    );
    assert!(!named.written);
    let never_named = host.render_texture_for_ui(unnamed);
    assert!(never_named.texture.is_none(), "no frame made it");
    assert!(!never_named.written);
    assert!(host.render_texture_for_ui(into).written);
}

#[test]
fn a_resized_texture_reads_unwritten_again() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (drawn, into) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    clear(&mut world, drawn);
    flush(&mut host, &mut world);
    assert!(host.render_texture_for_ui(drawn).written);

    let held = *world.render_texture(drawn).unwrap();
    world
        .update_render_texture(
            drawn,
            WorldRenderTexture {
                width: held.width * 2,
                ..held
            },
        )
        .unwrap();
    blit_from(&mut world, drawn, into);
    flush(&mut host, &mut world);

    let read = host.render_texture_for_ui(drawn);
    assert_eq!(read.texture.map(|t| t.width()), Some(held.width * 2));
    assert!(!read.written, "the new storage holds nothing a frame drew");
}

#[test]
fn a_depth_only_texture_is_refused_by_name() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let map = world
        .create_render_texture(WorldRenderTexture {
            width: SIDE,
            height: SIDE,
            format: 0,
            depth_bits: 32,
            mip_count: 1,
            srgb: false,
            dimension: dimension::TEX_2D,
            volume_depth: 1,
            auto_generate_mips: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::SetRenderTarget {
            color: Vec::new(),
            depth: Some(TargetRef::Render(map)),
            mip: 0,
            face: -1,
            slice: -1,
        })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::ClearRenderTarget {
            color: false,
            depth: true,
            stencil: false,
            value: [0.0; 4],
            depth_value: 1.0,
            stencil_value: 0,
        })
        .unwrap();
    flush(&mut host, &mut world);

    let read = host.render_texture_for_ui(map);

    let refused = UiTarget::new(map, read.texture, TextureSampler::default()).unwrap_err();
    assert_eq!(refused, UiTargetRefusal::DepthOrStencil);
    assert_eq!(refused.reason(), "depth-or-stencil");
}
