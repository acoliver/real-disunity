//! Making room in the runtime's storage budget: what no snapshot names is given back, least
//! recently named first, when an upload or the frame's render textures would not fit, and what the
//! snapshot names is never taken.
//!
//! The budget is the bytes of the textures the host uploaded and the render textures it made,
//! so each value below is a count of the texels of those textures.

use std::cell::RefCell;

use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureSampler};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime};
use worldrunner_render_program::unity::offscreen::OffscreenState;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, OffscreenWork, PropertyValue as WorldValue, TextureRef,
};

use super::super::adapter::OffscreenAdapter;
use super::super::assets::{AssetError, Assets};
use super::super::compute::ComputeAdapter;
use super::super::flush::OffscreenHost;
use super::super::frame::{Report, Settings};
use super::super::kernels::PlayerKernels;
use super::gpu::{Gpu, Quad, Synthetic, gpu_with_budget, shader};
use super::support::{material, render_texture, set, snapshot, texture_slot, world};

const A: u64 = 1 << 40;
const B: u64 = (1 << 40) + 1;
const C: u64 = (1 << 40) + 2;
/// A stored texture is 64 x 64 texels of 4 bytes.
const SIDE: u32 = 64;
const TEXTURE_BYTES: u64 = SIDE as u64 * SIDE as u64 * 4;
const TEXTURE_LEN: usize = (SIDE * SIDE * 4) as usize;

/// The quad's assets and three 64 x 64 textures under `A`, `B` and `C`, which note each time the
/// host reads one whole.
#[derive(Default)]
struct Textures {
    uploads: RefCell<Vec<u64>>,
}

impl Assets for Textures {
    fn mesh(&self, identity: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Quad.mesh(identity, known)
    }

    fn texture(&self, identity: u64, known: u64) -> Result<Option<PlayerTexture>, AssetError> {
        if ![A, B, C].contains(&identity) || known != 0 {
            return Ok(None);
        }
        self.uploads.borrow_mut().push(identity);
        Ok(Some(PlayerTexture {
            format: 4,
            width: SIDE,
            height: SIDE,
            mips: 1,
            linear: false,
            generation: 1,
            distance_spread: 0,
            bytes: vec![200; TEXTURE_LEN],
        }))
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// A runtime with a budget, a state and an adapter, over the device and arena it was made on.
struct Room {
    runtime: ProgramRuntime,
    state: OffscreenState,
    adapter: OffscreenAdapter,
    textures: Textures,
    _device: (wgpu::Device, SubmissionAuthority, ProgramArena),
}

impl Room {
    fn new(budget: u64) -> Self {
        let Gpu {
            device,
            authority,
            runtime,
            arena,
        } = gpu_with_budget(budget);
        let state = OffscreenState::new(&runtime);
        let settings = Settings {
            light_modes: vec!["Alpha".to_owned()],
            ..Settings::default()
        };
        Self {
            runtime,
            state,
            adapter: OffscreenAdapter::with_supply(Synthetic, settings),
            textures: Textures::default(),
            _device: (device, authority, arena),
        }
    }

    fn adapt(&mut self, snapshot: &GraphicsSnapshot) -> Report {
        self.adapter
            .adapt(snapshot, &self.textures, &mut self.state, &mut self.runtime)
            .report
    }
}

/// A snapshot of a world whose blit material names the stored textures `named` and whose render
/// target is `side` texels across.
fn naming(named: &[u64], side: u32) -> GraphicsSnapshot {
    let mut world = world();
    naming_in(&mut world, named, side);
    snapshot(&mut world)
}

fn naming_in(world: &mut World, named: &[u64], side: u32) {
    let blit = shader(world, "blit");
    let material = material(world, blit);
    for (index, identity) in named.iter().enumerate() {
        set(
            world,
            material,
            WorldValue::Texture(
                format!("_Stored{index}"),
                texture_slot(TextureRef::Asset(*identity)),
            ),
        );
    }
    let target = render_texture(world, [side, side]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(target),
            target,
            material: Some(material),
            pass: 0,
        })
        .unwrap();
}

/// What the runtime holds with nothing named: the default textures and buffers.
fn base_bytes() -> u64 {
    let mut room = Room::new(64 << 20);
    room.adapt(&naming(&[], 4));
    room.runtime.live_bytes()
}

fn over_budget(report: &Report) -> usize {
    report
        .causes
        .iter()
        .filter(|(name, _)| name.contains("OverBudget"))
        .map(|(_, count)| *count)
        .sum()
}

#[test]
fn a_texture_no_later_snapshot_names_is_given_back_when_the_next_upload_needs_the_room() {
    let base = base_bytes();
    // Room for one stored texture, and nothing like a second.
    let mut room = Room::new(base + TEXTURE_BYTES + 1024);
    let first = room.adapt(&naming(&[A], 4));
    assert_eq!(over_budget(&first), 0, "{:?}", first.causes);
    assert_eq!(room.runtime.live_bytes(), base + TEXTURE_BYTES);

    let second = room.adapt(&naming(&[B], 4));
    assert_eq!(over_budget(&second), 0, "{:?}", second.causes);
    assert_eq!(
        (
            second.reclaimed.textures,
            second.reclaimed.meshes,
            second.reclaimed.bytes
        ),
        (1, 0, TEXTURE_BYTES)
    );
    room.runtime.collect();
    assert_eq!(room.runtime.live_bytes(), base + TEXTURE_BYTES);

    // The first texture was forgotten, so naming it again uploads it again, in B's place.
    let third = room.adapt(&naming(&[A], 4));
    assert_eq!(over_budget(&third), 0, "{:?}", third.causes);
    assert_eq!(third.causes.get("texture:NotStored"), None);
    assert_eq!(third.reclaimed.textures, 1);
}

#[test]
fn textures_a_snapshot_names_are_never_given_back_for_each_other() {
    let base = base_bytes();
    let mut room = Room::new(base + TEXTURE_BYTES + 1024);
    // Both are named, only one fits, and the one that fits is named too.
    let report = room.adapt(&naming(&[A, B], 4));
    assert_eq!(over_budget(&report), 1, "{:?}", report.causes);
    assert_eq!(report.reclaimed.textures, 0);
    assert_eq!(room.runtime.live_bytes(), base + TEXTURE_BYTES);
}

#[test]
fn the_least_recently_named_texture_goes_first() {
    let base = base_bytes();
    // Room for two stored textures.
    let mut room = Room::new(base + 2 * TEXTURE_BYTES + 1024);
    room.adapt(&naming(&[A], 4));
    room.adapt(&naming(&[B], 4));
    // Naming A again makes B the one named longer ago.
    room.adapt(&naming(&[A], 4));
    assert_eq!(
        *room.textures.uploads.borrow(),
        [A, B],
        "A is held, not read again"
    );

    // C does not fit beside A and B, so the host gives back B, not A.
    let report = room.adapt(&naming(&[C], 4));
    assert_eq!(over_budget(&report), 0, "{:?}", report.causes);
    assert_eq!(
        (report.reclaimed.textures, report.reclaimed.bytes),
        (1, TEXTURE_BYTES)
    );
    room.adapt(&naming(&[A], 4));
    assert_eq!(
        *room.textures.uploads.borrow(),
        [A, B, C],
        "A was kept and is not read again"
    );
}

#[test]
fn the_render_textures_of_a_frame_take_the_room_of_what_no_snapshot_names() {
    let base = base_bytes();
    // The first frame holds the stored texture, a 4 x 4 target of 64 bytes, the 32 bytes of the
    // default vertex values the first build makes and the 4096 bytes of the constants buffer,
    // which stay. The second needs a 64 x 64 target of 16384 bytes, and
    // only the 4096 bytes of slack and the 64 of the first target are free.
    let (small_target, constants, big_target) = (4 * 4 * 4, 4096 + 32, TEXTURE_BYTES);
    let Gpu {
        device: _device,
        authority: _authority,
        runtime,
        arena,
    } = gpu_with_budget(base + TEXTURE_BYTES + small_target + constants + 4096);
    let directory = tempfile::tempdir().unwrap();
    let settings = Settings {
        light_modes: vec!["Alpha".to_owned()],
        ..Settings::default()
    };
    let mut host = OffscreenHost::new(
        runtime,
        arena,
        OffscreenAdapter::with_supply(Synthetic, settings),
        ComputeAdapter::new(PlayerKernels::new(directory.path())),
    );

    let (_, first) = host.frame(&naming(&[A], 4), &Textures::default()).unwrap();
    host.settle();
    assert_eq!(over_budget(&first), 0, "{:?}", first.causes);
    assert_eq!(
        host.live_bytes(),
        base + TEXTURE_BYTES + small_target + constants
    );

    let (_, second) = host.frame(&naming(&[], 64), &Textures::default()).unwrap();
    host.settle();
    assert_eq!(over_budget(&second), 0, "{:?}", second.causes);
    assert_eq!(
        (
            second.reclaimed.textures,
            second.reclaimed.meshes,
            second.reclaimed.bytes
        ),
        (1, 0, TEXTURE_BYTES)
    );
    assert_eq!(host.live_bytes(), base + big_target + constants);
}
