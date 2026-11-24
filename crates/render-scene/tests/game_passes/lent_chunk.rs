//! A lent level that is not first in the shared chunks it is packed into is drawn from where it
//! starts there, not from the start of the chunk.
use super::fixture::{Rig, SIZE, quad_scene};
use super::lent::{ClearKept, KEPT, Scenario, inventory, output};
use worldrunner_render_plan::{
    MeshHandle, RenderScene, SceneGeometry, SceneMesh, SceneMeshLod, SceneParts, SceneVertex,
};
use worldrunner_render_scene::compositor::FrameSceneMeshes;

/// A square from `west` to `west + 3` metres and three metres either side of the middle, as
/// corners in the order lower left, lower right, upper right, upper left.
fn square(west_millimeters: i64, indices: Vec<u32>) -> SceneMesh {
    let (west, east) = (west_millimeters, west_millimeters + 3_000);
    let corners = [
        ([west, -3_000, 0], [0, 0]),
        ([east, -3_000, 0], [1_000_000, 0]),
        ([east, 3_000, 0], [1_000_000, 1_000_000]),
        ([west, 3_000, 0], [0, 1_000_000]),
    ];
    let vertices = corners
        .into_iter()
        .map(|(position, uv)| SceneVertex {
            tangent_micro: None,
            position_millimeters: position,
            normal_micro: [0, 0, 1_000_000],
            texcoord_micro: uv,
        })
        .collect();
    SceneMesh::new(vec![SceneMeshLod::new(0, vertices, indices, None).unwrap()]).unwrap()
}

/// Two meshes uploaded together, so they share a vertex chunk and an index chunk. The first is a
/// square left of the middle. The second is a square right of it whose indices are the upper
/// left triangle and then the lower right one, so each part of it is drawn by indices that
/// differ from the first mesh's at the same place in the chunk.
fn two_mesh_scene() -> RenderScene {
    let quad = quad_scene();
    let meshes = vec![
        square(-4_000, vec![0, 1, 2, 0, 2, 3]),
        square(1_000, vec![0, 2, 3, 0, 1, 2]),
    ];
    RenderScene::new(SceneParts {
        resources: quad.resources().clone(),
        geometry: SceneGeometry::new(meshes, quad.geometry().objects().to_vec()).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

/// Column and row of each texel the game's draw covers.
fn covered(image: &[[u8; 4]]) -> Vec<(usize, usize)> {
    let width = SIZE[0] as usize;
    image
        .iter()
        .enumerate()
        .filter(|(_, texel)| texel[3] == 255)
        .map(|(at, _)| (at % width, at / width))
        .collect()
}

#[test]
fn a_game_draws_a_lent_level_that_starts_after_another_in_the_shared_chunks() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let (game_passes, _) = inventory(Scenario::Chunk);
    let mut rig = Rig::new(&two_mesh_scene(), SIZE, |settings| {
        settings.game_passes = game_passes;
    });
    rig.render().unwrap();

    // The game drew the lower right triangle of the second square: from its lower left corner
    // (column 38, row 42) to its lower right (56, 42) and its upper right (56, 6), at six pixels
    // to the metre about the middle of the frame, column 32 and row 24.
    let image = output(&rig);
    let drawn = covered(&image);
    assert!(
        (300..=350).contains(&drawn.len()),
        "half of an 18 by 36 pixel square is covered: {}",
        drawn.len()
    );
    assert!(
        drawn
            .iter()
            .all(|&(column, row)| column >= 38 && row + 2 * (column - 38) >= 40),
        "only the lower right half of the second square is covered"
    );
    assert_eq!(
        Rig::pixel(&image, 52, 38)[3],
        255,
        "lower right of the square"
    );
    assert_eq!(Rig::pixel(&image, 40, 10)[3], 0, "upper left of the square");
    assert_eq!(
        Rig::pixel(&image, 14, 38)[3],
        0,
        "the first mesh's square, left of the middle, is not drawn"
    );
    assert_eq!(Rig::pixel(&image, 14, 10)[3], 0, "nor is its upper half");

    let (first, second) = (MeshHandle::from_raw(0), MeshHandle::from_raw(1));
    let lent: FrameSceneMeshes = KEPT.with_borrow(|kept| kept.snapshots[0].clone());
    let (vertices, other_vertices) = (
        lent.vertex_input(second, 0).unwrap(),
        lent.vertex_input(first, 0).unwrap(),
    );
    let whole = lent.indices(second, 0, 0..6).unwrap();
    let other = lent.indices(first, 0, 0..6).unwrap();
    assert_eq!(
        vertices.buffer, other_vertices.buffer,
        "both meshes' vertices are in one chunk"
    );
    assert_eq!(
        whole.buffer, other.buffer,
        "both meshes' indices are in one chunk"
    );
    assert_eq!(
        (other.first, other.base_vertex),
        (0, 0),
        "the first mesh starts the chunks"
    );
    assert_eq!(
        (whole.first, whole.base_vertex),
        (6, 4),
        "the second mesh starts after the first mesh's six indices and four vertices"
    );
    let part = lent.indices(second, 0, 3..6).unwrap();
    assert_eq!(
        (part.first, part.count, part.base_vertex),
        (9, 3, 4),
        "a range of the level is offset from the level's start"
    );
}
