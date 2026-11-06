use super::*;
use crate::geometry::VERTEX_BYTES;
use glam::{Vec2, Vec3};
use worldrunner_render_plan::{
    MaterialHandle, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
    TerrainLodPolicy, TerrainStreamingPolicy,
};

pub(crate) fn fixture(lod_policy: TerrainLodPolicy) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 12,
        rows: 10,
        origin_east_millimeters: -1_234_567,
        origin_north_millimeters: 7_654_321,
        sample_spacing_millimeters: 733,
        heights_millimeters: (0..120).map(|i| (i * i * 107) % 7311 - 2517).collect(),
        surface_indices: vec![0; 120],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy,
    })
    .unwrap()
}

fn word(bytes: &[u8], index: usize) -> u32 {
    u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
}

fn point(source: &[u8], columns: u32, sample: [u32; 2]) -> Vec3 {
    let x = usize::try_from(sample[0]).unwrap();
    let y = usize::try_from(sample[1]).unwrap();
    Vec3::new(
        f32::from_bits(word(source, word(source, 0) as usize + x)),
        f32::from_bits(word(source, word(source, 1) as usize + y)),
        f32::from_bits(word(
            source,
            word(source, 2) as usize + y * columns as usize + x,
        )),
    )
}

fn mesh_point(mesh: &super::super::TerrainMesh, index: u32) -> Vec3 {
    let offset = index as usize * VERTEX_BYTES;
    Vec3::from_array(std::array::from_fn(|axis| {
        f32::from_le_bytes(
            mesh.vertices[offset + axis * 4..offset + axis * 4 + 4]
                .try_into()
                .unwrap(),
        )
    }))
}

#[test]
fn source_positions_and_face_planes_match_mesh_bits_at_every_lod() {
    let streamed = fixture(TerrainLodPolicy::FullResolution)
        .with_streaming_policy(Some(TerrainStreamingPolicy::new(2, 100, 16, 1000).unwrap()));
    for terrain in [
        fixture(TerrainLodPolicy::FullResolution),
        fixture(TerrainLodPolicy::UniformSteps { steps: 2 }),
        streamed,
    ] {
        let source = HeightfieldPlanes::new(&terrain);
        assert_eq!(source.chunks, chunk_indices(&terrain));
        let coordinates = terrain.columns() + terrain.rows() + terrain.columns() * terrain.rows();
        assert_eq!(
            source.bytes.len(),
            16 + 4 * coordinates as usize + 32 * source.chunks.len()
        );
        for (chunk, index) in &source.chunks {
            let mesh = if terrain.streaming_policy().is_some() {
                super::super::chunks::build(&terrain, *chunk).unwrap()
            } else {
                super::super::build_terrain_mesh(&terrain).unwrap()
            };
            verify_chunk(&terrain, &source.bytes, *index, &mesh);
        }
    }
}

fn verify_chunk(
    terrain: &SceneTerrain,
    bytes: &[u8],
    index: u32,
    mesh: &super::super::TerrainMesh,
) {
    let start = word(bytes, 3) as usize + index as usize * 8;
    let chunk = TerrainChunk {
        minimum: [word(bytes, start), word(bytes, start + 1)],
        maximum: [word(bytes, start + 2), word(bytes, start + 3)],
        stride: word(bytes, start + 4),
    };
    let width = word(bytes, start + 5);
    let height = word(bytes, start + 6);
    let triangles = (width - 1) * (height - 1) * 2;
    for (number, triangle) in mesh
        .indices
        .chunks_exact(12)
        .take(triangles as usize)
        .enumerate()
    {
        let ids: [u32; 3] = std::array::from_fn(|i| {
            u32::from_le_bytes(triangle[i * 4..i * 4 + 4].try_into().unwrap())
        });
        let positions = ids.map(|i| mesh_point(mesh, i));
        for (id, position) in ids.into_iter().zip(positions) {
            let sample = [
                (chunk.minimum[0] + id % width * chunk.stride).min(chunk.maximum[0]),
                (chunk.minimum[1] + id / width * chunk.stride).min(chunk.maximum[1]),
            ];
            assert_eq!(
                point(bytes, terrain.columns(), sample)
                    .to_array()
                    .map(f32::to_bits),
                position.to_array().map(f32::to_bits)
            );
        }
        verify_plane(
            bytes,
            terrain.columns(),
            chunk,
            width,
            u32::try_from(number).unwrap(),
            positions,
        );
    }
}

fn verify_plane(
    bytes: &[u8],
    columns: u32,
    chunk: TerrainChunk,
    width: u32,
    number: u32,
    positions: [Vec3; 3],
) {
    let centroid = (positions[0] + positions[1] + positions[2]) / 3.0;
    let cell = [
        chunk.minimum[0] + (number / 2 % (width - 1)) * chunk.stride,
        chunk.minimum[1] + (number / 2 / (width - 1)) * chunk.stride,
    ];
    let hi = [
        (cell[0] + chunk.stride).min(chunk.maximum[0]),
        (cell[1] + chunk.stride).min(chunk.maximum[1]),
    ];
    let a = point(bytes, columns, cell);
    let c = point(bytes, columns, hi);
    let fraction = (centroid.truncate() - a.truncate()) / (c - a).truncate();
    let normal = if fraction.y <= fraction.x {
        (point(bytes, columns, [hi[0], cell[1]]) - a).cross(c - a)
    } else {
        (c - a).cross(point(bytes, columns, [cell[0], hi[1]]) - a)
    };
    let expected = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
    assert_eq!(
        normal.to_array().map(f32::to_bits),
        expected.to_array().map(f32::to_bits)
    );
    assert!(fraction.cmpgt(Vec2::ZERO).all() && fraction.cmplt(Vec2::ONE).all());
}

#[test]
fn terrain_plane_instance_preserves_identity_matrix_and_encodes_full_ids() {
    let original = crate::geometry::pack_instance(&super::super::TERRAIN_PLACEMENT);
    for index in [0, 1, 0x10000, u32::MAX] {
        let bytes = instance_bytes(index);
        assert_eq!(&bytes[..64], &original[..64]);
        let low = f32::from_le_bytes(bytes[64..68].try_into().unwrap());
        let high = f32::from_le_bytes(bytes[68..72].try_into().unwrap());
        assert!(low.is_normal() || low == 0.0);
        assert!(high.is_normal() || high == 0.0);
        let expected_low = f32::from(u16::try_from(index & 0xffff).unwrap());
        let expected_high = f32::from(u16::try_from(index >> 16).unwrap());
        assert_eq!(
            [low, high].map(f32::to_bits),
            [expected_low, expected_high].map(f32::to_bits)
        );
    }
}

#[test]
fn terrain_plane_upload_checks_buffer_and_storage_binding_limits() {
    for limits in [
        wgpu::Limits {
            max_buffer_size: 128,
            ..Default::default()
        },
        wgpu::Limits {
            max_storage_buffer_binding_size: 128,
            ..Default::default()
        },
    ] {
        assert!(super::validate_size(128, &limits).is_ok());
        assert!(matches!(
            super::validate_size(132, &limits),
            Err(crate::draw::DrawError::TerrainPlaneLimit {
                required: 132,
                limit: 128
            })
        ));
    }
}

#[test]
fn terrain_instance_limit_is_independent_of_triangle_storage() {
    let limits = wgpu::Limits {
        max_buffer_size: 256,
        max_storage_buffer_binding_size: 256,
        ..Default::default()
    };
    assert!(super::validate_size(240, &limits).is_ok());
    let result = super::instance_size(4, &limits);
    assert!(
        result.is_err(),
        "320 instance bytes exceed the 256-byte buffer limit"
    );
    assert_eq!(
        result.unwrap_err().to_string(),
        "terrain instances need 320 bytes, device buffer limit is 256"
    );
    assert_eq!(super::instance_size(3, &limits).unwrap(), 240);
    let vertex_only = wgpu::Limits {
        max_storage_buffer_binding_size: 1,
        ..limits
    };
    assert_eq!(super::instance_size(3, &vertex_only).unwrap(), 240);
}

#[test]
fn terrain_instance_byte_requirement_rejects_overflow() {
    assert!(super::instance_size(usize::MAX, &wgpu::Limits::default()).is_err());
}

#[test]
fn terrain_instance_limit_rejects_large_tree_with_admitted_triangle_source() {
    let limits = wgpu::Limits::default();
    let count = 3_495_253;
    let source_bytes = 16 + 4 * (1537 + 1537 + 1537 * 1537) + 32 * count;
    assert_eq!(source_bytes, 121_309_884);
    assert!(super::validate_size(source_bytes as u64, &limits).is_ok());
    assert!(matches!(
        super::instance_size(count, &limits),
        Err(crate::draw::DrawError::TerrainInstanceLimit {
            required: 279_620_240,
            limit: 268_435_456,
        })
    ));
}
