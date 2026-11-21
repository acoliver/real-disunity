use worldrunner_render_plan::{
    MaterialHandle, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    TerrainLodPolicy,
};
use worldrunner_render_scene::terrain::{TerrainError, build_terrain_chunk_mesh};
use worldrunner_render_scene::{geometry::VERTEX_STRIDE, terrain::streaming::TerrainChunk};

fn terrain() -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 7,
        rows: 4,
        origin_east_millimeters: -2_000,
        origin_north_millimeters: 3_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: vec![2_000; 28],
        surface_indices: vec![0; 28],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

const CHUNK: TerrainChunk = TerrainChunk {
    minimum: [1, 0],
    maximum: [6, 3],
    stride: 2,
};

#[test]
fn stock_mesh_includes_the_far_edges_and_double_sided_skirts() {
    let mesh = build_terrain_chunk_mesh(&terrain(), CHUNK).unwrap();
    let xy = [
        [-1.0_f32, 1.0, 3.0, 4.0].as_slice(),
        [3.0_f32, 5.0, 6.0].as_slice(),
    ];
    let points: Vec<_> = mesh
        .vertices
        .chunks_exact(usize::try_from(VERTEX_STRIDE).unwrap())
        .map(|vertex| {
            std::array::from_fn::<_, 3, _>(|i| {
                u32::from_le_bytes(vertex[4 * i..4 * i + 4].try_into().unwrap())
            })
        })
        .collect();
    let expected: Vec<_> = xy[1]
        .iter()
        .flat_map(|&y| {
            xy[0]
                .iter()
                .map(move |&x| [x, y, 2.0_f32].map(f32::to_bits))
        })
        .collect();
    assert_eq!(&points[..12], expected);
    assert_eq!(points.len(), 32);
    assert!(
        points[12..]
            .iter()
            .all(|position| position[2] == 1.0_f32.to_bits())
    );
    let indices: Vec<_> = mesh
        .indices
        .chunks_exact(4)
        .map(|i| u32::from_le_bytes(i.try_into().unwrap()))
        .collect();
    assert_eq!(mesh.index_count, 156);
    assert_eq!(indices.len(), 156);
    assert_eq!(&indices[..6], &[0, 1, 5, 0, 5, 4]);
    assert_eq!(
        &indices[36..48],
        &[0, 12, 1, 1, 12, 13, 1, 12, 0, 13, 12, 1]
    );
    assert!(indices.iter().all(|index| *index < 32));
}

#[test]
fn invalid_caller_chunk_keys_fail_before_sampling() {
    let terrain = terrain();
    for key in [
        TerrainChunk { stride: 0, ..CHUNK },
        TerrainChunk { stride: 3, ..CHUNK },
        TerrainChunk {
            minimum: [7, 0],
            ..CHUNK
        },
        TerrainChunk {
            maximum: [7, 3],
            ..CHUNK
        },
        TerrainChunk {
            maximum: [6, 0],
            ..CHUNK
        },
    ] {
        assert_eq!(
            build_terrain_chunk_mesh(&terrain, key),
            Err(TerrainError::InvalidChunk),
            "{key:?}"
        );
    }
}
