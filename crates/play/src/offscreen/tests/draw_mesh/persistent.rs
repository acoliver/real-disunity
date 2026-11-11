//! Geometry and sub-mesh generations across flushes of the same host.

use super::{
    BLUE, GREEN, LEFT, MESH, Mat4, MaterialId, MeshChannels, Meshes, OTHER_MESH, OffscreenHost,
    PlayerKernels, RED, RIGHT_QUARTER, RenderTextureId, Run, Skip, World, bind_and_clear,
    draw_mesh, flat, flush_in, host, host_with_budget, identity_view, left_half, mesh_of, scene,
    stored,
};

fn next_draw(
    host: &mut OffscreenHost,
    world: &mut World,
    target: RenderTextureId,
    material: MaterialId,
    meshes: &Meshes,
    submesh: u32,
) -> Run {
    bind_and_clear(world, target);
    identity_view(world);
    draw_mesh(world, (MESH, Mat4::IDENTITY), material, (submesh, 0));
    flush_in(host, world, target, meshes)
}

fn replace(meshes: &mut Meshes, mut mesh: MeshChannels, generation: u64) {
    mesh.generation = generation;
    meshes.0.insert(MESH, mesh);
}

#[test]
fn one_host_refreshes_changed_geometry_ranges_and_empty_meshes() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(PlayerKernels::new(directory.path()));
    let (mut world, target) = scene();
    let material = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let mut meshes = stored([(MESH, left_half())]);

    let first = next_draw(&mut host, &mut world, target, material, &meshes, 0);
    assert_eq!(first.columns(), [RED, RED, GREEN, GREEN]);
    assert!(first.outcome.report.skipped.is_empty());

    replace(&mut meshes, mesh_of(&[RIGHT_QUARTER]), 1);
    let unchanged = next_draw(&mut host, &mut world, target, material, &meshes, 0);
    assert_eq!(unchanged.columns(), [RED, RED, GREEN, GREEN]);
    assert!(unchanged.outcome.report.skipped.is_empty());

    replace(&mut meshes, mesh_of(&[LEFT, RIGHT_QUARTER]), 2);
    let changed = next_draw(&mut host, &mut world, target, material, &meshes, 1);
    assert_eq!(changed.columns(), [GREEN, GREEN, GREEN, RED]);
    assert!(changed.outcome.report.skipped.is_empty());

    replace(&mut meshes, mesh_of(&[]), 3);
    let empty = next_draw(&mut host, &mut world, target, material, &meshes, 0);
    assert_eq!(empty.columns(), [GREEN; 4]);
    assert_eq!(empty.outcome.report.skipped[&Skip::EmptyMesh], 1);
    assert!(empty.outcome.report.failures.is_empty());

    replace(&mut meshes, left_half(), 4);
    let restored = next_draw(&mut host, &mut world, target, material, &meshes, 0);
    assert_eq!(restored.columns(), [RED, RED, GREEN, GREEN]);
    assert!(restored.outcome.report.skipped.is_empty());

    let removed = next_draw(&mut host, &mut world, target, material, &meshes, 1);
    assert_eq!(removed.columns(), [GREEN; 4]);
    assert_eq!(removed.outcome.report.skipped[&Skip::NoSuchSubmesh], 1);
    assert!(removed.outcome.report.failures.is_empty());
}

/// The mesh with `PADDING` vertices appended that no index names, which makes its storage far
/// larger than the staging of a flush.
fn padded(mut mesh: MeshChannels) -> MeshChannels {
    mesh.vertex_count += PADDING;
    for channel in &mut mesh.channels {
        channel
            .data
            .resize(mesh.vertex_count as usize * channel.dimension as usize, 0.0);
    }
    mesh
}

const PADDING: u32 = 2048;
/// The bytes of the positions of the padding alone, which the padded mesh's storage exceeds.
const PADDING_POSITION_BYTES: u64 = PADDING as u64 * 12;

#[test]
fn a_mesh_refused_for_room_draws_when_the_room_returns_without_a_new_generation() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host_with_budget(PlayerKernels::new(directory.path()), 1 << 20);
    let (mut world, target) = scene();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let blue = flat(&mut world, [0.0, 0.0, 1.0, 1.0]);
    let meshes = stored([
        (OTHER_MESH, left_half()),
        (MESH, padded(mesh_of(&[RIGHT_QUARTER]))),
    ]);
    let draw = |world: &mut World, big: bool| {
        bind_and_clear(world, target);
        identity_view(world);
        draw_mesh(world, (OTHER_MESH, Mat4::IDENTITY), red, (0, 0));
        if big {
            draw_mesh(world, (MESH, Mat4::IDENTITY), blue, (0, 0));
        }
    };

    // The small mesh is uploaded and drawn alone first; it stays named below, so it cannot be
    // given back for the large one. What this flush needed beyond what it left is the room a
    // flush takes besides meshes.
    draw(&mut world, false);
    let warm = flush_in(&mut host, &mut world, target, &meshes);
    assert_eq!(warm.columns(), [RED, RED, GREEN, GREEN]);
    let staging = host.peak_bytes() - host.live_bytes();
    let free = staging + 256;
    assert!(free < PADDING_POSITION_BYTES);

    // Everything else in the budget is taken, so the large mesh does not fit and the flush does.
    let taken = usize::try_from(host.storage_budget() - host.live_bytes() - free).unwrap();
    let filler = host
        .runtime_mut()
        .create_buffer("filler", &vec![0; taken])
        .unwrap();
    draw(&mut world, true);
    let refused = flush_in(&mut host, &mut world, target, &meshes);
    assert_eq!(refused.columns(), [RED, RED, GREEN, GREEN]);
    assert_eq!(refused.outcome.report.skipped[&Skip::MeshUnavailable], 1);
    assert_eq!(
        refused
            .outcome
            .report
            .causes
            .iter()
            .filter(|(name, _)| name.starts_with("mesh:") && name.contains("OverBudget"))
            .count(),
        1,
        "{:?}",
        refused.outcome.report.causes
    );

    // The room returns while the engine still holds the same generation of the mesh.
    host.runtime_mut().release_buffer(filler).unwrap();
    host.settle();
    draw(&mut world, true);
    let relieved = flush_in(&mut host, &mut world, target, &meshes);
    assert_eq!(relieved.columns(), [RED, RED, GREEN, BLUE]);
    assert!(relieved.outcome.report.skipped.is_empty());
    assert!(relieved.outcome.report.failures.is_empty());
}
