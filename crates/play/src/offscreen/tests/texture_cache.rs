//! The adapter's stored-texture cache across snapshots of one persistent adapter: what an upload
//! the budget refused does on the next snapshot, and what a texture that changes to a malformed
//! one or to one that does not fit leaves in the runtime's storage.
//!
//! The storage is counted in the runtime's budget, so each test reads it back as `live_bytes`
//! once the GPU has finished with what was released. A stored texture is 64 x 64 texels of four
//! bytes, far more than anything else a snapshot here allocates. A snapshot names a texture as
//! the source of a blit, so a texture the frame cannot read shows as a blit left out.

use std::cell::RefCell;
use std::collections::BTreeMap;

use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureSampler};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, OffscreenWork, RenderTextureId, TargetRef, TextureRef,
};

use super::super::assets::{AssetError, Assets};
use super::super::frame::{Report, Skip};
use super::flush::host_with_budget;
use super::gpu::{Quad, target};
use super::reclaim::Room;
use super::support::{render_texture, snapshot, world};

const A: u64 = 1 << 40;
const B: u64 = (1 << 40) + 1;
/// An identity the engine never stores.
const ABSENT: u64 = (1 << 40) + 2;
const SIDE: u32 = 64;
const TEXTURE_BYTES: u64 = SIDE as u64 * SIDE as u64 * 4;
const TEXTURE_LEN: usize = (SIDE * SIDE * 4) as usize;
/// `GraphicsFormat` 8, four 8-bit channels.
const ARGB32: u32 = 8;

/// The textures the engine stores, by identity, and each read the host made of them as
/// `(identity, known generation)`. As the engine does, a read of a generation that is still
/// current gives nothing.
#[derive(Default)]
struct Engine {
    stored: RefCell<BTreeMap<u64, PlayerTexture>>,
    reads: RefCell<Vec<(u64, u64)>>,
}

impl Engine {
    fn store(&self, identity: u64, texture: PlayerTexture) {
        self.stored.borrow_mut().insert(identity, texture);
    }

    fn reads_of(&self, identity: u64) -> Vec<u64> {
        self.reads
            .borrow()
            .iter()
            .filter(|(read, _)| *read == identity)
            .map(|(_, known)| *known)
            .collect()
    }
}

impl Assets for Engine {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Quad.mesh(identity, known)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        self.reads.borrow_mut().push((identity, known));
        Ok(self
            .stored
            .borrow()
            .get(&identity)
            .filter(|texture| texture.generation != known)
            .cloned())
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// A linear RGBA32 texture of `SIDE` by `SIDE` texels of `texel` at `generation`.
fn filled(generation: u64, texel: [u8; 4]) -> PlayerTexture {
    PlayerTexture {
        format: 4,
        width: SIDE,
        height: SIDE,
        mips: 1,
        linear: true,
        generation,
        distance_spread: 0,
        bytes: texel.repeat(TEXTURE_LEN / 4),
    }
}

fn whole(generation: u64) -> PlayerTexture {
    filled(generation, [200; 4])
}

/// The texture with one byte fewer than its base level needs.
fn short(generation: u64) -> PlayerTexture {
    let mut texture = whole(generation);
    texture.bytes.pop();
    texture
}

/// The snapshot of a world that blits each of `textures` into one render target.
fn blitting(textures: &[u64]) -> GraphicsSnapshot {
    let mut world = world();
    let drawn = render_texture(&mut world, [4, 4]);
    for &identity in textures {
        world
            .record_offscreen(OffscreenWork::Blit {
                source: TextureRef::Asset(identity),
                target: drawn,
                material: None,
                pass: 0,
            })
            .unwrap();
    }
    snapshot(&mut world)
}

impl Room {
    /// Maps the snapshot against `engine`, then waits for the GPU and collects what was released.
    fn adapt_textures(&mut self, engine: &Engine, snapshot: &GraphicsSnapshot) -> Report {
        let adapted = self
            .adapter
            .adapt(snapshot, engine, &mut self.state, &mut self.runtime);
        let _ = self
            .runtime
            .device()
            .poll(wgpu::PollType::wait_indefinitely());
        self.runtime.collect();
        adapted.report
    }
}

/// The blits a report mapped and the blits it left out because the frame cannot read their source.
fn blits(report: &Report) -> (usize, usize) {
    (
        report.built_by_kind.get("Blit").copied().unwrap_or(0),
        report
            .skipped
            .get(&Skip::SourceUnavailable)
            .copied()
            .unwrap_or(0),
    )
}

fn over_budget(report: &Report) -> usize {
    report
        .causes
        .iter()
        .filter(|(name, _)| name.starts_with("texture:") && name.contains("OverBudget"))
        .map(|(_, count)| *count)
        .sum()
}

/// The bytes a room holds after a snapshot whose only texture the engine does not store.
fn base_bytes() -> u64 {
    let mut room = Room::new(64 << 20);
    let report = room.adapt_textures(&Engine::default(), &blitting(&[ABSENT]));
    assert_eq!(blits(&report), (0, 1));
    room.runtime.live_bytes()
}

#[test]
fn a_texture_the_budget_refused_uploads_when_room_appears_though_the_engine_has_no_newer_one() {
    let base = base_bytes();
    // One texture fits, and a second does not fit beside it.
    let mut room = Room::new(base + TEXTURE_BYTES + 1024);
    let engine = Engine::default();
    engine.store(A, whole(1));
    engine.store(B, whole(1));

    let first = room.adapt_textures(&engine, &blitting(&[A]));
    assert_eq!(blits(&first), (1, 0), "{:?}", first.failures);
    assert_eq!(room.runtime.live_bytes(), base + TEXTURE_BYTES);

    // Both are named, so the first cannot be given back for the second.
    let refused = room.adapt_textures(&engine, &blitting(&[A, B]));
    assert_eq!(over_budget(&refused), 1, "{:?}", refused.causes);
    assert_eq!(blits(&refused), (1, 1));

    // The next snapshot names only the second. Nothing about the engine's texture changed, and
    // the first is no longer named, so its room is given back for the second.
    let relieved = room.adapt_textures(&engine, &blitting(&[B]));
    assert!(relieved.failures.is_empty(), "{:?}", relieved.failures);
    assert_eq!(blits(&relieved), (1, 0));
    assert_eq!(relieved.reclaimed.textures, 1);
    assert_eq!(room.runtime.live_bytes(), base + TEXTURE_BYTES);
}

#[test]
fn a_refused_texture_is_read_again_in_full_and_a_malformed_one_is_not_read_again() {
    let base = base_bytes();
    let mut room = Room::new(base + TEXTURE_BYTES + 1024);
    let engine = Engine::default();
    engine.store(A, whole(7));
    engine.store(B, whole(9));
    room.adapt_textures(&engine, &blitting(&[A]));
    room.adapt_textures(&engine, &blitting(&[A, B]));
    room.adapt_textures(&engine, &blitting(&[A, B]));
    // Refused for the budget each time, so the host asks for the whole texture each time.
    assert_eq!(engine.reads_of(B), [0, 0]);

    // Malformed, the engine's texture is the same until it changes, and reading it again would
    // fail the same way.
    engine.store(B, short(10));
    let mut room = Room::new(64 << 20);
    let reports: Vec<_> = (0..3)
        .map(|_| room.adapt_textures(&engine, &blitting(&[B])))
        .collect();
    assert_eq!(reports[0].failures.get("texture:Short"), Some(&1));
    assert!(reports[1].failures.is_empty(), "{:?}", reports[1].failures);
    assert!(reports[2].failures.is_empty(), "{:?}", reports[2].failures);
    assert_eq!(engine.reads_of(B)[2..], [0, 10, 10]);
    assert!(reports.iter().all(|report| blits(report) == (0, 1)));
}

#[test]
fn a_texture_that_changes_to_a_malformed_one_gives_back_its_storage_and_a_later_one_recovers() {
    let base = base_bytes();
    let mut room = Room::new(64 << 20);
    let engine = Engine::default();
    engine.store(A, whole(1));
    room.adapt_textures(&engine, &blitting(&[A]));
    assert_eq!(room.runtime.live_bytes(), base + TEXTURE_BYTES);

    engine.store(A, short(2));
    let malformed = room.adapt_textures(&engine, &blitting(&[A]));
    assert_eq!(malformed.failures.get("texture:Short"), Some(&1));
    assert_eq!(blits(&malformed), (0, 1));
    assert_eq!(
        room.runtime.live_bytes(),
        base,
        "the old texture's storage is given back"
    );

    engine.store(A, whole(3));
    let recovered = room.adapt_textures(&engine, &blitting(&[A]));
    assert!(recovered.failures.is_empty(), "{:?}", recovered.failures);
    assert_eq!(blits(&recovered), (1, 0));
    assert_eq!(room.runtime.live_bytes(), base + TEXTURE_BYTES);
}

#[test]
fn a_replacement_the_budget_refuses_gives_back_the_old_texture_and_uploads_on_the_next_snapshot() {
    let base = base_bytes();
    // Room for one texture, and not for a replacement beside the texture it replaces.
    let mut room = Room::new(base + TEXTURE_BYTES + TEXTURE_BYTES / 2);
    let engine = Engine::default();
    engine.store(A, whole(1));
    room.adapt_textures(&engine, &blitting(&[A]));

    engine.store(A, whole(2));
    let refused = room.adapt_textures(&engine, &blitting(&[A]));
    assert_eq!(over_budget(&refused), 1, "{:?}", refused.causes);
    assert_eq!(blits(&refused), (0, 1), "the old texels are not read");
    assert_eq!(
        room.runtime.live_bytes(),
        base,
        "the old texture is not stranded"
    );

    let retried = room.adapt_textures(&engine, &blitting(&[A]));
    assert!(retried.failures.is_empty(), "{:?}", retried.failures);
    assert_eq!(blits(&retried), (1, 0));
    assert_eq!(room.runtime.live_bytes(), base + TEXTURE_BYTES);
    assert_eq!(engine.reads_of(A), [0, 1, 0]);
}

/// Clears `drawn` to green, blits the stored texture `A` over it and asks for its texels.
fn clear_blit_and_read(world: &mut World, drawn: RenderTextureId) -> u64 {
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
        OffscreenWork::Blit {
            source: TextureRef::Asset(A),
            target: drawn,
            material: None,
            pass: 0,
        },
    ] {
        world.record_offscreen(work).unwrap();
    }
    world.request_texture_pixels(drawn, 0, 0, 4, 4).unwrap()
}

#[test]
fn a_texture_refused_for_room_is_read_by_a_blit_when_the_room_returns_without_a_new_generation() {
    const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with_budget(directory.path(), 1 << 20);
    let engine = Engine::default();
    let mut world = world();
    let drawn = target(&mut world, ARGB32);

    // A flush before the engine stores the texture shows what a flush takes besides one.
    let request = clear_blit_and_read(&mut world, drawn);
    let warm = host.flush(&snapshot(&mut world), &engine).unwrap();
    warm.apply(&mut world).unwrap();
    assert_eq!(world.take_texture_pixels(request), Some(GREEN.repeat(16)));
    let staging = host.peak_bytes() - host.live_bytes();
    let free = staging + 256;
    assert!(free < TEXTURE_BYTES);

    // Everything else in the budget is taken, so the texture does not fit and the flush does.
    // The texture is one colour, so a blit of it into a smaller target gives that colour in
    // every texel whatever filter samples it.
    engine.store(A, filled(1, [255, 0, 0, 255]));
    let taken = usize::try_from(host.storage_budget() - host.live_bytes() - free).unwrap();
    let filler = host
        .runtime_mut()
        .create_buffer("filler", &vec![0; taken])
        .unwrap();
    let request = clear_blit_and_read(&mut world, drawn);
    let refused = host.flush(&snapshot(&mut world), &engine).unwrap();
    refused.apply(&mut world).unwrap();
    assert_eq!(world.take_texture_pixels(request), Some(GREEN.repeat(16)));
    assert_eq!(
        over_budget(&refused.report),
        1,
        "{:?}",
        refused.report.causes
    );
    assert_eq!(blits(&refused.report).1, 1);

    // The room returns while the engine still holds the same generation of the texture.
    host.runtime_mut().release_buffer(filler).unwrap();
    host.settle();
    let request = clear_blit_and_read(&mut world, drawn);
    let relieved = host.flush(&snapshot(&mut world), &engine).unwrap();
    relieved.apply(&mut world).unwrap();
    assert_eq!(world.take_texture_pixels(request), Some(RED.repeat(16)));
    assert!(
        relieved.report.failures.is_empty(),
        "{:?}",
        relieved.report.failures
    );
}
