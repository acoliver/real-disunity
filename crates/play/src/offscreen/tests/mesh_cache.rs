//! The adapter's mesh cache across snapshots of one persistent adapter: what an upload the budget
//! refused does on the next snapshot, and what a mesh that changes to nothing to draw, to a
//! malformed one or to one that does not fit leaves in the runtime's storage.
//!
//! The storage is counted in the runtime's budget, so each test reads it back as `live_bytes`
//! once the GPU has finished with what was released. The meshes are synthetic quads padded with
//! unused vertices so that their storage is far larger than anything else a snapshot allocates.

use std::cell::RefCell;
use std::collections::BTreeMap;

use worldrunner_disunity_runtime::{MeshChannels, PlayerTexture, TextureSampler};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, MaterialId, OffscreenWork, RenderTextureId, TargetRef,
};

use super::super::assets::{AssetError, Assets};
use super::super::frame::Report;
use super::gpu::{MESH, Quad, shader};
use super::reclaim::Room;
use super::support::{material, render_texture, snapshot, world};

const OTHER: u64 = MESH + 1;
/// Vertices a mesh holds beyond the quad's four, which no index names.
const PADDING: u32 = 2048;

/// The meshes the engine stores, by identity, and each read the host made of them as
/// `(identity, known generation)`. As the engine does, a read of a generation that is still
/// current gives nothing.
#[derive(Default)]
struct Engine {
    stored: RefCell<BTreeMap<u64, MeshChannels>>,
    reads: RefCell<Vec<(u64, u64)>>,
}

impl Engine {
    fn store(&self, identity: u64, mesh: MeshChannels) {
        self.stored.borrow_mut().insert(identity, mesh);
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
        self.reads.borrow_mut().push((identity, known));
        Ok(self
            .stored
            .borrow()
            .get(&identity)
            .filter(|mesh| mesh.generation != known)
            .cloned())
    }

    fn texture(&self, _: u64, _: u64) -> Result<Option<PlayerTexture>, AssetError> {
        Ok(None)
    }

    fn sampler(&self, _: u64) -> Result<Option<TextureSampler>, AssetError> {
        Ok(None)
    }
}

/// The quad at `generation`, followed by `PADDING` vertices at the origin that nothing indexes.
fn padded(generation: u64) -> MeshChannels {
    let mut mesh = Quad.mesh(MESH, 0).unwrap().unwrap();
    mesh.generation = generation;
    mesh.vertex_count += PADDING;
    for channel in &mut mesh.channels {
        channel
            .data
            .resize(mesh.vertex_count as usize * channel.dimension as usize, 0.0);
    }
    mesh
}

/// A mesh with no vertices, indices or sub-meshes.
fn nothing(generation: u64) -> MeshChannels {
    MeshChannels {
        generation,
        vertex_count: 0,
        channels: Vec::new(),
        indices: Vec::new(),
        ranges: Vec::new(),
    }
}

/// A mesh whose sub-mesh draws from indices but which has no position channel.
fn without_positions(generation: u64) -> MeshChannels {
    let mut mesh = padded(generation);
    mesh.channels.retain(|channel| channel.number != 0);
    mesh
}

/// A world with a render target and a material, whose snapshots draw the meshes they name.
struct Scene {
    world: World,
    target: RenderTextureId,
    material: MaterialId,
}

impl Scene {
    fn new() -> Self {
        let mut world = world();
        let shader = shader(&mut world, "camera");
        let material = material(&mut world, shader);
        let target = render_texture(&mut world, [4, 4]);
        Self {
            world,
            target,
            material,
        }
    }

    /// The snapshot of a command buffer that draws each of `meshes` once.
    fn drawing(&mut self, meshes: &[u64]) -> GraphicsSnapshot {
        let color = vec![TargetRef::Render(self.target)];
        self.world
            .record_offscreen(OffscreenWork::SetRenderTarget {
                color,
                depth: None,
                mip: 0,
                face: -1,
                slice: -1,
            })
            .unwrap();
        for &mesh in meshes {
            self.world
                .record_offscreen(OffscreenWork::DrawMesh {
                    mesh,
                    matrix: glam::Mat4::IDENTITY.to_cols_array(),
                    material: self.material,
                    submesh: 0,
                    pass: 0,
                    block: None,
                })
                .unwrap();
        }
        snapshot(&mut self.world)
    }
}

impl Room {
    /// Maps the snapshot against `engine`, then waits for the GPU and collects what was released.
    fn adapt_against(&mut self, engine: &Engine, snapshot: &GraphicsSnapshot) -> Report {
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

    fn live(&self) -> u64 {
        self.runtime.live_bytes()
    }
}

fn draws(report: &Report) -> usize {
    report.built_by_kind.get("DrawMesh").copied().unwrap_or(0)
}

fn over_budget(report: &Report) -> usize {
    report
        .causes
        .iter()
        .filter(|(name, _)| name.starts_with("mesh:") && name.contains("OverBudget"))
        .map(|(_, count)| *count)
        .sum()
}

/// The bytes a room holds with no mesh named, and the bytes one padded mesh takes.
fn sizes() -> (u64, u64) {
    let mut room = Room::new(64 << 20);
    let engine = Engine::default();
    engine.store(MESH, padded(1));
    let mut scene = Scene::new();
    room.adapt_against(&engine, &scene.drawing(&[]));
    let base = room.live();
    let report = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert_eq!(draws(&report), 1, "{:?}", report.failures);
    (base, room.live() - base)
}

#[test]
fn a_mesh_the_budget_refused_uploads_when_room_appears_though_the_engine_has_no_newer_generation() {
    let (base, mesh) = sizes();
    // One mesh fits, and a second does not fit beside it.
    let mut room = Room::new(base + mesh + mesh / 2);
    let engine = Engine::default();
    engine.store(MESH, padded(1));
    engine.store(OTHER, padded(1));
    let mut scene = Scene::new();

    let first = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert_eq!(draws(&first), 1, "{:?}", first.failures);

    // Both are named, so the first cannot be given back for the second.
    let refused = room.adapt_against(&engine, &scene.drawing(&[MESH, OTHER]));
    assert_eq!(over_budget(&refused), 1, "{:?}", refused.causes);
    assert_eq!(draws(&refused), 1);
    assert_eq!(room.adapter.mesh_metadata_counts(), (1, 1, 1));

    // The next snapshot names only the second. Nothing about the engine's mesh changed, and the
    // first mesh is no longer named, so its room is given back for the second.
    let relieved = room.adapt_against(&engine, &scene.drawing(&[OTHER]));
    assert!(relieved.failures.is_empty(), "{:?}", relieved.failures);
    assert_eq!(draws(&relieved), 1);
    assert_eq!(relieved.reclaimed.meshes, 1);
    assert_eq!(room.adapter.mesh_metadata_counts(), (1, 1, 1));
    assert_eq!(room.live(), base + mesh);
}

#[test]
fn a_refused_mesh_is_read_again_in_full_and_a_malformed_one_is_not_read_again() {
    let (base, mesh) = sizes();
    let mut room = Room::new(base + mesh + mesh / 2);
    let engine = Engine::default();
    engine.store(MESH, padded(7));
    engine.store(OTHER, padded(9));
    let mut scene = Scene::new();
    room.adapt_against(&engine, &scene.drawing(&[MESH]));
    room.adapt_against(&engine, &scene.drawing(&[MESH, OTHER]));
    room.adapt_against(&engine, &scene.drawing(&[MESH, OTHER]));
    // Refused for the budget each time, so the host asks for the whole mesh each time.
    assert_eq!(engine.reads_of(OTHER), [0, 0]);

    // Malformed, the engine's mesh is the same until it changes, and reading it again would
    // fail the same way.
    engine.store(OTHER, without_positions(10));
    let mut room = Room::new(64 << 20);
    let reports: Vec<_> = (0..3)
        .map(|_| room.adapt_against(&engine, &scene.drawing(&[OTHER])))
        .collect();
    assert_eq!(reports[0].failures.values().sum::<usize>(), 1);
    assert!(reports[1].failures.is_empty(), "{:?}", reports[1].failures);
    assert!(reports[2].failures.is_empty(), "{:?}", reports[2].failures);
    assert_eq!(engine.reads_of(OTHER)[2..], [0, 10, 10]);
}

#[test]
fn a_mesh_that_changes_to_nothing_to_draw_gives_back_its_storage_and_a_later_mesh_recovers() {
    let (base, mesh) = sizes();
    let mut room = Room::new(64 << 20);
    let engine = Engine::default();
    let mut scene = Scene::new();
    engine.store(MESH, padded(1));
    let first = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert_eq!(draws(&first), 1);
    assert_eq!(room.live(), base + mesh);
    assert_eq!(room.adapter.mesh_metadata_counts(), (1, 1, 1));

    engine.store(MESH, nothing(2));
    let emptied = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert!(emptied.failures.is_empty(), "{:?}", emptied.failures);
    assert_eq!(draws(&emptied), 0);
    assert_eq!(room.live(), base, "the old mesh's storage is given back");
    assert_eq!(room.adapter.mesh_metadata_counts(), (1, 0, 0));

    // Nothing names the empty mesh any more, so nothing of it is kept.
    room.adapt_against(&engine, &scene.drawing(&[]));
    assert_eq!(room.adapter.mesh_metadata_counts(), (0, 0, 0));

    engine.store(MESH, padded(3));
    let recovered = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert!(recovered.failures.is_empty(), "{:?}", recovered.failures);
    assert_eq!(draws(&recovered), 1);
    assert_eq!(room.live(), base + mesh);
    assert_eq!(room.adapter.mesh_metadata_counts(), (1, 1, 1));
}

#[test]
fn a_mesh_that_changes_to_a_malformed_one_gives_back_its_storage_and_a_later_mesh_recovers() {
    let (base, mesh) = sizes();
    let mut room = Room::new(64 << 20);
    let engine = Engine::default();
    let mut scene = Scene::new();
    engine.store(MESH, padded(1));
    room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert_eq!(room.live(), base + mesh);

    engine.store(MESH, without_positions(2));
    let malformed = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert_eq!(malformed.failures.values().sum::<usize>(), 1);
    assert_eq!(draws(&malformed), 0);
    assert_eq!(room.live(), base, "the old mesh's storage is given back");
    assert_eq!(room.adapter.mesh_metadata_counts(), (1, 0, 0));

    engine.store(MESH, padded(3));
    let recovered = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert!(recovered.failures.is_empty(), "{:?}", recovered.failures);
    assert_eq!(draws(&recovered), 1);
    assert_eq!(room.live(), base + mesh);
    assert_eq!(room.adapter.mesh_metadata_counts(), (1, 1, 1));
}

#[test]
fn a_replacement_the_budget_refuses_gives_back_the_old_mesh_and_uploads_on_the_next_snapshot() {
    let (base, mesh) = sizes();
    // Room for one mesh, and not for a replacement beside the mesh it replaces.
    let mut room = Room::new(base + mesh + mesh / 2);
    let engine = Engine::default();
    let mut scene = Scene::new();
    engine.store(MESH, padded(1));
    room.adapt_against(&engine, &scene.drawing(&[MESH]));

    engine.store(MESH, padded(2));
    let refused = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert_eq!(over_budget(&refused), 1, "{:?}", refused.causes);
    assert_eq!(draws(&refused), 0, "the old geometry is not drawn");
    assert_eq!(room.live(), base, "the old mesh is not stranded");
    assert_eq!(room.adapter.mesh_metadata_counts(), (0, 0, 0));

    let retried = room.adapt_against(&engine, &scene.drawing(&[MESH]));
    assert!(retried.failures.is_empty(), "{:?}", retried.failures);
    assert_eq!(draws(&retried), 1);
    assert_eq!(room.live(), base + mesh);
    assert_eq!(room.adapter.mesh_metadata_counts(), (1, 1, 1));
}
