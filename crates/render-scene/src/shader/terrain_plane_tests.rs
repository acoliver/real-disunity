use super::gpu_math_tests::evaluate_source;
use crate::terrain::{TerrainMesh, planes::HeightfieldPlanes};
use glam::{Vec2, Vec3};
use std::fmt::Write;
use worldrunner_render_plan::{SceneTerrain, TerrainLodPolicy, TerrainStreamingPolicy};

struct Case {
    point: Vec2,
    index: u32,
    axis: Vec2,
    normal: Vec3,
}

#[test]
fn terrain_gpu_heightfield_planes_match_original_mesh_at_every_lod_and_skirt() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context =
        crate::device::GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    let terrain = crate::terrain::planes::tests::fixture(TerrainLodPolicy::FullResolution)
        .with_streaming_policy(Some(TerrainStreamingPolicy::new(2, 100, 16, 1000).unwrap()));
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
        "incorrect GPU triangle planes: {result:?}"
    );
    assert_eq!(
        result[2].to_bits(),
        0,
        "GPU source positions differ from mesh bits: {result:?}"
    );
    assert!(result[1] < 2e-6, "GPU normal error: {result:?}");
    assert!(result[3] > 100.0);
}

fn mesh_point(mesh: &TerrainMesh, id: u32) -> Vec3 {
    let offset = id as usize * crate::geometry::VERTEX_BYTES;
    Vec3::from_array(std::array::from_fn(|a| {
        f32::from_le_bytes(
            mesh.vertices[offset + a * 4..offset + a * 4 + 4]
                .try_into()
                .unwrap(),
        )
    }))
}

fn skirt_axis(id: u32, width: u32, height: u32) -> Vec2 {
    if id < width * height {
        return Vec2::ZERO;
    }
    let edge_vertex = id - width * height;
    let south = 2 * (width - 1);
    let east = south + 2 * (height - 1);
    if (south..east).contains(&edge_vertex) || edge_vertex >= east + south {
        Vec2::X
    } else {
        Vec2::Y
    }
}

fn cases(terrain: &SceneTerrain, planes: &HeightfieldPlanes) -> Vec<Case> {
    let mut cases = Vec::new();
    for (chunk, index) in &planes.chunks {
        let mesh = crate::terrain::chunks::build(terrain, *chunk).unwrap();
        let width = (chunk.maximum[0] - chunk.minimum[0]).div_ceil(chunk.stride) + 1;
        let height = (chunk.maximum[1] - chunk.minimum[1]).div_ceil(chunk.stride) + 1;
        for triangle in mesh.indices.chunks_exact(12) {
            let ids: [u32; 3] = std::array::from_fn(|i| {
                u32::from_le_bytes(triangle[i * 4..i * 4 + 4].try_into().unwrap())
            });
            let points = ids.map(|id| mesh_point(&mesh, id));
            let centre = (points[0] + points[1] + points[2]) / 3.0;
            let normal = (points[1] - points[0])
                .cross(points[2] - points[0])
                .normalize();
            let axis = ids
                .map(|id| skirt_axis(id, width, height))
                .into_iter()
                .sum::<Vec2>()
                / 3.0;
            cases.push(Case {
                point: centre.truncate(),
                index: *index,
                axis,
                normal: if axis == Vec2::ZERO {
                    normal
                } else {
                    normal.abs()
                },
            });
        }
    }
    cases
}

fn entry(terrain: &SceneTerrain, planes: &HeightfieldPlanes) -> String {
    let cases = cases(terrain, planes);
    let points = cases
        .iter()
        .map(|c| {
            format!(
                "vec4<f32>({},{},{},{})",
                c.point.x, c.point.y, c.axis.x, c.axis.y
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let indices = cases
        .iter()
        .map(|c| format!("{}u", c.index))
        .collect::<Vec<_>>()
        .join(",");
    let normals = cases
        .iter()
        .map(|c| format!("vec3<f32>({},{},{})", c.normal.x, c.normal.y, c.normal.z))
        .collect::<Vec<_>>()
        .join(",");
    let count = cases.len();
    let origin = [
        terrain.origin_east_millimeters(),
        terrain.origin_north_millimeters(),
        i64::from(terrain.sample_spacing_millimeters()),
    ]
    .map(crate::geometry::millimeter_to_meter);
    let checks = position_checks(terrain);
    format!(
        r"@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    terrain.origin_and_spacing = vec4<f32>({},{},{},0.1);
    terrain.dimensions_and_count = vec4<u32>({}u,{}u,1u,0u);
    let points = array<vec4<f32>,{count}>({points});
    let indices = array<u32,{count}>({indices});
    let normals = array<vec3<f32>,{count}>({normals});
    var failures = 0u; var position_failures = 0u; var maximum_error = 0.0;
    for (var i = 0u; i < {count}u; i++) {{
        let actual = normalize(terrain_face_normal(points[i].xy, indices[i], points[i].zw));
        let error = length(actual - normals[i]);
        maximum_error = max(maximum_error, error);
        failures += select(0u, 1u, error > 2e-6);
    }}
    {checks}
    result[0] = vec4<f32>(f32(failures), maximum_error, f32(position_failures), {count}.0);
}}",
        origin[0],
        origin[1],
        origin[2],
        terrain.columns(),
        terrain.rows()
    )
}

fn position_checks(terrain: &SceneTerrain) -> String {
    let mut checks = String::new();
    for y in 0..terrain.rows() {
        for x in 0..terrain.columns() {
            let east = crate::geometry::millimeter_to_meter(
                terrain.origin_east_millimeters()
                    + i64::from(x) * i64::from(terrain.sample_spacing_millimeters()),
            );
            let north = crate::geometry::millimeter_to_meter(
                terrain.origin_north_millimeters()
                    + i64::from(y) * i64::from(terrain.sample_spacing_millimeters()),
            );
            let up = crate::geometry::millimeter_to_meter(i64::from(
                terrain.heights_millimeters()[(y * terrain.columns() + x) as usize],
            ));
            writeln!(checks, "position_failures += select(0u, 1u, any(bitcast<vec3<u32>>(terrain_sample_position(vec2<u32>({x}u,{y}u))) != vec3<u32>({}u,{}u,{}u)));", east.to_bits(), north.to_bits(), up.to_bits()).unwrap();
        }
    }
    checks
}
