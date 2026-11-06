use super::gpu_math_tests::evaluate_source;
use crate::terrain::{planes::HeightfieldPlanes, streaming::TerrainChunk};
use glam::Vec2;
use worldrunner_render_plan::{
    MaterialHandle, SceneTerrain, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    TerrainLodPolicy, TerrainStreamingPolicy,
};

struct Case {
    point: Vec2,
    index: u32,
    cell: [u32; 2],
}

fn boundary_terrain() -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 4,
        rows: 2,
        origin_east_millimeters: -1000,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 333,
        heights_millimeters: vec![0, 0, 1000, 1000, 0, 0, 1000, 1000],
        surface_indices: vec![0; 8],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

#[test]
fn terrain_gpu_cells_follow_stored_boundaries_and_outer_edge_ownership() {
    let context = pollster::block_on(crate::device::GpuContext::request_with_features(
        wgpu::Features::empty(),
    ))
    .unwrap();
    let fixture = crate::terrain::planes::tests::fixture;
    for terrain in [
        boundary_terrain(),
        fixture(TerrainLodPolicy::FullResolution),
        fixture(TerrainLodPolicy::UniformSteps { steps: 2 }),
        fixture(TerrainLodPolicy::FullResolution)
            .with_streaming_policy(Some(TerrainStreamingPolicy::new(2, 100, 16, 1000).unwrap())),
    ] {
        let planes = HeightfieldPlanes::new(&terrain);
        let source = super::TERRAIN_WGSL
            .replace("@group(1) @binding(23)", "@group(1) @binding(7)")
            .replace(
                "@group(1) @binding(0) var<uniform> terrain: Terrain;",
                "var<private> terrain: Terrain;",
            );
        let words: Vec<_> = planes
            .bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let result = evaluate_source(
            &context,
            &source,
            &entry(&terrain, &planes),
            None,
            Some(&words),
        );
        assert_eq!(
            result[0].to_bits(),
            0,
            "wrong GPU cell ownership: {result:?}"
        );
        assert!(result[1] > 10.0);
    }
}

fn coordinates(terrain: &SceneTerrain, chunk: TerrainChunk, axis: usize) -> Vec<f32> {
    let origins = [
        terrain.origin_east_millimeters(),
        terrain.origin_north_millimeters(),
    ];
    let count = (chunk.maximum[axis] - chunk.minimum[axis]).div_ceil(chunk.stride);
    (0..=count)
        .map(|cell| {
            let sample = (chunk.minimum[axis] + cell * chunk.stride).min(chunk.maximum[axis]);
            crate::geometry::millimeter_to_meter(
                origins[axis] + i64::from(sample) * i64::from(terrain.sample_spacing_millimeters()),
            )
        })
        .collect()
}

fn cases(terrain: &SceneTerrain, planes: &HeightfieldPlanes) -> Vec<Case> {
    let mut cases = Vec::new();
    for (chunk, index) in &planes.chunks {
        let coordinates = [
            coordinates(terrain, *chunk, 0),
            coordinates(terrain, *chunk, 1),
        ];
        for axis in 0..2 {
            for boundary in &coordinates[axis] {
                for position in [boundary.next_down(), *boundary, boundary.next_up()] {
                    let mut point = Vec2::new(
                        coordinates[0][0].midpoint(coordinates[0][1]),
                        coordinates[1][0].midpoint(coordinates[1][1]),
                    );
                    point[axis] = position;
                    let cell = std::array::from_fn(|i| {
                        let interval = coordinates[i].partition_point(|v| *v <= point[i]);
                        u32::try_from(interval.saturating_sub(1).min(coordinates[i].len() - 2))
                            .unwrap()
                    });
                    cases.push(Case {
                        point,
                        index: *index,
                        cell,
                    });
                }
            }
        }
    }
    if terrain.columns() == 4 {
        cases.push(Case {
            point: Vec2::new(f32::from_bits(0xbeab_020d), 0.1),
            index: 0,
            cell: [1, 0],
        });
    }
    cases
}

fn entry(terrain: &SceneTerrain, planes: &HeightfieldPlanes) -> String {
    let cases = cases(terrain, planes);
    let points = cases
        .iter()
        .map(|c| {
            format!(
                "vec2<f32>(bitcast<f32>({}u),bitcast<f32>({}u))",
                c.point.x.to_bits(),
                c.point.y.to_bits(),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let expected = cases
        .iter()
        .map(|c| format!("vec3<u32>({}u,{}u,{}u)", c.index, c.cell[0], c.cell[1]))
        .collect::<Vec<_>>()
        .join(",");
    let count = cases.len();
    let origin = [
        terrain.origin_east_millimeters(),
        terrain.origin_north_millimeters(),
        i64::from(terrain.sample_spacing_millimeters()),
    ]
    .map(crate::geometry::millimeter_to_meter);
    format!(
        r"@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    terrain.origin_and_spacing = vec4<f32>({},{},{},1.0);
    terrain.dimensions_and_count = vec4<u32>({}u,{}u,1u,0u);
    let points = array<vec2<f32>,{count}>({points});
    let expected = array<vec3<u32>,{count}>({expected});
    var failures = 0u;
    for (var i = 0u; i < {count}u; i++) {{
        let descriptor = terrain_plane_data[3] + expected[i].x * 8u;
        let cell = terrain_cell(points[i], descriptor);
        failures += select(0u, 1u, any(cell != expected[i].yz));
    }}
    result[0] = vec4<f32>(f32(failures), {count}.0, 0.0, 0.0);
}}",
        origin[0],
        origin[1],
        origin[2],
        terrain.columns(),
        terrain.rows()
    )
}
