//! Making room in the runtime's storage budget: what no snapshot names is given back, least
//! recently named first, when an upload or the frame's render textures would not fit, and what the
//! snapshot names is never taken.
//!
//! The budget is the bytes of the textures the host uploaded and the render textures it made,
//! so each value below is a count of the texels of those textures.

use std::cell::RefCell;

use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureSampler};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::runtime::{ProgramArena, ProgramRuntime};
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::OffscreenState;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, OffscreenWork, PropertyValue as WorldValue, RenderTextureId, TextureRef,
};

use super::super::adapter::OffscreenAdapter;
use super::super::assets::{AssetError, Assets};
use super::super::compute::ComputeAdapter;
use super::super::flush::OffscreenHost;
use super::super::frame::{Reclaimed, Report, Settings};
use super::super::ids;
use super::super::kernels::PlayerKernels;
use super::gpu::{Gpu, MESH, Quad, Synthetic, gpu_with_budget, shader};
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
    device: (wgpu::Device, SubmissionAuthority, ProgramArena),
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
            device: (device, authority, arena),
        }
    }

    /// Maps the snapshot, makes room for its frame as the host does, builds it and waits for the
    /// GPU, so that what was given back no longer counts.
    fn frame(&mut self, snapshot: &GraphicsSnapshot) -> (Result<(), OffscreenError>, Reclaimed) {
        let adapted =
            self.adapter
                .adapt(snapshot, &self.textures, &mut self.state, &mut self.runtime);
        let reclaimed = self
            .adapter
            .make_room(&mut self.state, &mut self.runtime, &adapted.frame);
        let built = self
            .state
            .build(&mut self.runtime, &self.device.2, &adapted.frame)
            .map(drop);
        let _ = self
            .runtime
            .device()
            .poll(wgpu::PollType::wait_indefinitely());
        self.runtime.collect();
        (built, reclaimed)
    }

    /// The render textures the state holds, by the ids the world gave them.
    fn held(&self) -> Vec<TextureId> {
        self.state
            .render_textures_held()
            .into_iter()
            .map(|(id, _)| id)
            .collect()
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

/// A world with three render textures: `a` and `b` of 8 x 8 texels (256 bytes each) and `c` of
/// 64 x 64 (16384 bytes).
struct Targets {
    world: World,
    a: RenderTextureId,
    b: RenderTextureId,
    c: RenderTextureId,
}

impl Targets {
    fn new() -> Self {
        let mut world = world();
        let a = render_texture(&mut world, [8, 8]);
        let b = render_texture(&mut world, [8, 8]);
        let c = render_texture(&mut world, [64, 64]);
        Self { world, a, b, c }
    }

    /// The snapshot of a frame whose records draw into the render textures `named` and no other.
    fn naming(&mut self, named: &[RenderTextureId]) -> GraphicsSnapshot {
        self.naming_held(named, &[])
    }

    /// As `naming`, with a property of the frame's material holding the render textures `held`.
    fn naming_held(
        &mut self,
        named: &[RenderTextureId],
        held: &[RenderTextureId],
    ) -> GraphicsSnapshot {
        let blit = shader(&mut self.world, "blit");
        let material = material(&mut self.world, blit);
        for (index, &texture) in held.iter().enumerate() {
            set(
                &mut self.world,
                material,
                WorldValue::Texture(
                    format!("_Held{index}"),
                    texture_slot(TextureRef::Render(texture)),
                ),
            );
        }
        for &target in named {
            self.world
                .record_offscreen(OffscreenWork::Blit {
                    source: TextureRef::Render(target),
                    target,
                    material: Some(material),
                    pass: 0,
                })
                .unwrap();
        }
        snapshot(&mut self.world)
    }

    fn id(id: RenderTextureId) -> TextureId {
        ids::render_texture(id)
    }
}

/// The bytes the runtime holds after the frames of `Targets` that name `a` and `b`, then `b`,
/// with room to spare: the budget a tight room needs to be built from.
fn held_after_two_frames() -> u64 {
    let mut room = Room::new(64 << 20);
    let mut targets = Targets::new();
    let (first, second) = (
        targets.naming(&[targets.a, targets.b]),
        targets.naming(&[targets.b]),
    );
    room.frame(&first).0.unwrap();
    room.frame(&second).0.unwrap();
    room.runtime.live_bytes()
}

#[test]
fn the_render_texture_named_longest_ago_is_given_back_first_when_the_frame_needs_the_room() {
    let held = held_after_two_frames();
    // `c` fits in the budget only when 256 bytes are given back.
    let mut room = Room::new(held + 16384 - 256);
    let mut targets = Targets::new();
    let (a, b, c) = (targets.a, targets.b, targets.c);
    // `b` is named in the first frame and `a` in the second, so `b` was named longer ago.
    let frames = [
        targets.naming(&[a, b]),
        targets.naming(&[a]),
        targets.naming(&[c]),
    ];
    room.frame(&frames[0]).0.unwrap();
    room.frame(&frames[1]).0.unwrap();
    assert_eq!(room.held(), [Targets::id(a), Targets::id(b)]);

    let (built, reclaimed) = room.frame(&frames[2]);
    built.unwrap();
    assert_eq!(
        (reclaimed.targets, reclaimed.textures, reclaimed.bytes),
        (1, 0, 256)
    );
    assert_eq!(room.held(), [Targets::id(a), Targets::id(c)]);
}

#[test]
fn a_render_texture_the_snapshot_names_is_not_given_back_though_it_was_named_longest_ago() {
    let held = held_after_two_frames();
    let mut room = Room::new(held + 16384 - 256);
    let mut targets = Targets::new();
    let (a, b, c) = (targets.a, targets.b, targets.c);
    // `a` was named first and `b` since, and the third frame names `a` and `c` but not `b`.
    let frames = [
        targets.naming(&[a, b]),
        targets.naming(&[b]),
        targets.naming(&[a, c]),
    ];
    room.frame(&frames[0]).0.unwrap();
    room.frame(&frames[1]).0.unwrap();

    let (built, reclaimed) = room.frame(&frames[2]);
    built.unwrap();
    assert_eq!(reclaimed.targets, 1);
    assert_eq!(room.held(), [Targets::id(a), Targets::id(c)]);
}

#[test]
fn when_the_snapshot_names_every_render_texture_none_is_given_back_and_the_frame_is_refused() {
    let held = held_after_two_frames();
    let mut room = Room::new(held + 16384 - 256);
    let mut targets = Targets::new();
    let (a, b, c) = (targets.a, targets.b, targets.c);
    let frames = [targets.naming(&[a, b]), targets.naming(&[a, b, c])];
    room.frame(&frames[0]).0.unwrap();

    let (built, reclaimed) = room.frame(&frames[1]);
    assert!(
        matches!(
            built,
            Err(OffscreenError::Program(ProgramError::OverBudget { .. }))
        ),
        "{built:?}"
    );
    assert_eq!(reclaimed, Reclaimed::default());
    assert_eq!(room.held(), [Targets::id(a), Targets::id(b)]);
}

#[test]
fn a_render_texture_a_material_property_holds_is_named_and_stays() {
    let held = held_after_two_frames();
    let mut room = Room::new(held + 16384 - 256);
    let mut targets = Targets::new();
    let (a, b, c) = (targets.a, targets.b, targets.c);
    // `b` was named longest ago, but the third frame's material holds it; no record draws into it.
    let frames = [
        targets.naming(&[a, b]),
        targets.naming(&[a]),
        targets.naming_held(&[c], &[b]),
    ];
    room.frame(&frames[0]).0.unwrap();
    room.frame(&frames[1]).0.unwrap();

    let (built, reclaimed) = room.frame(&frames[2]);
    built.unwrap();
    assert_eq!(reclaimed.targets, 1);
    assert_eq!(room.held(), [Targets::id(b), Targets::id(c)]);
}

struct Meshes;

impl Assets for Meshes {
    fn mesh(&self, _: u64, known: u64) -> Result<Option<MeshChannels>, AssetError> {
        Quad.mesh(MESH, known)
    }

    fn texture(&self, _: u64, _: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok(None)
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

#[test]
fn evicting_meshes_releases_their_metadata_and_later_draws_reload_them() {
    let mut room = Room::new(64 << 20);
    let mut world = world();
    let shader = shader(&mut world, "camera");
    let material = material(&mut world, shader);
    let target = render_texture(&mut world, [4, 4]);
    for identity in (MESH..MESH + 16).chain(std::iter::once(MESH)) {
        world
            .record_offscreen(OffscreenWork::SetRenderTarget {
                color: vec![worldrunner_world_simulation::graphics::TargetRef::Render(
                    target,
                )],
                depth: None,
                mip: 0,
                face: -1,
                slice: -1,
            })
            .unwrap();
        world
            .record_offscreen(OffscreenWork::DrawMesh {
                mesh: identity,
                matrix: glam::Mat4::IDENTITY.to_cols_array(),
                material,
                submesh: 0,
                pass: 0,
            })
            .unwrap();
        let drawing = snapshot(&mut world);
        let adapted = room
            .adapter
            .adapt(&drawing, &Meshes, &mut room.state, &mut room.runtime);
        assert!(
            adapted.report.failures.is_empty(),
            "{:?}",
            adapted.report.failures
        );
        assert_eq!(adapted.report.built_by_kind["DrawMesh"], 1);
        assert_eq!(room.adapter.mesh_metadata_counts(), (1, 1, 1));
        let budget = room.runtime.budget().0;
        let protected = room
            .adapter
            .make_room_for(&mut room.state, &mut room.runtime, budget);
        assert_eq!(protected.meshes, 0);
        drop(adapted);

        room.adapt(&snapshot(&mut world));
        let reclaimed = room
            .adapter
            .make_room_for(&mut room.state, &mut room.runtime, budget);
        assert_eq!(reclaimed.meshes, 1);
        assert!(reclaimed.bytes > 0);
        assert_eq!(room.adapter.mesh_metadata_counts(), (0, 0, 0));
    }
}
