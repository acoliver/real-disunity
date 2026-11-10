//! Geometry and sub-mesh generations across flushes of the same host.

use super::{
    GREEN, LEFT, MESH, Mat4, MaterialId, MeshChannels, Meshes, OffscreenHost, PlayerKernels, RED,
    RIGHT_QUARTER, RenderTextureId, Run, Skip, World, bind_and_clear, draw_mesh, flat, flush_in,
    host, identity_view, left_half, mesh_of, scene, stored,
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
