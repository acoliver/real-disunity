#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    graphics::StreamingInfo,
    mesh::{
        Aabb, BakedCollision, BlendShape, BlendShapeChannel, BlendShapeData, BlendShapeVertex,
        ChannelInfo, CompressedMesh, Mesh, MeshLodInfo, PackedBitVector, Skin, SubMesh, VertexData,
    },
    write::ObjectWriter,
};

const BOX: [f32; 6] = [1.0, 2.0, 3.0, 0.5, 0.5, 0.5];

fn write_shapes(w: &mut ObjectWriter) {
    w.i32(1)
        .floats(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])
        .u32(3);
    w.i32(1).u32(0).u32(1).u8(1).u8(0).align();
    w.i32(1).string("Smile").unwrap().u32(77).i32(0).i32(1);
    w.i32(1).f32(100.0);
}

fn write_skin(w: &mut ObjectWriter) {
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    w.i32(1).floats(&identity);
    w.i32(2).u32(11).u32(12).u32(13);
    w.i32(1).floats(&[-1.0, -1.0, -1.0, 1.0, 1.0, 1.0]);
    w.i32(2).u32(4).u32(5);
}

/// A packed vector with one data byte; float vectors also carry range and start.
fn write_packed(w: &mut ObjectWriter, floats: bool) {
    w.u32(1);
    if floats {
        w.floats(&[2.0, -1.0]);
    }
    w.byte_array(&[0xab]).unwrap().u8(8).align();
}

fn write_compressed(w: &mut ObjectWriter) {
    for floats in [
        true, true, true, true, false, false, false, true, false, false,
    ] {
        write_packed(w, floats);
    }
    w.u32(9);
}

/// The mesh with compression 2, readable, without kept vertices, with kept indices.
fn mesh_bytes(big_endian: bool) -> Vec<u8> {
    mesh_bytes_with(big_endian, [2, 1, 0, 1])
}

/// A mesh whose compression and three keep flags are `flags`.
fn mesh_bytes_with(big_endian: bool, flags: [u8; 4]) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("Hex").unwrap();
    w.i32(1)
        .u32(2)
        .u32(6)
        .i32(1)
        .u32(3)
        .u32(5)
        .u32(4)
        .floats(&BOX);
    write_shapes(&mut w);
    write_skin(&mut w);
    w.bytes(&flags).align().i32(0);
    w.byte_array(&[0, 0, 1, 0, 2, 0]).unwrap();
    w.u32(4).i32(2).bytes(&[0, 0, 0, 3, 1, 12, 2, 3]);
    w.byte_array(&[9, 9, 9, 9, 9]).unwrap().align();
    write_compressed(&mut w);
    w.floats(&BOX)
        .i32(1)
        .i32(30)
        .byte_array(&[1])
        .unwrap()
        .byte_array(&[])
        .unwrap();
    w.floats(&[1.0, 0.5])
        .u64(512)
        .u32(96)
        .string("resources.assets.resS")
        .unwrap();
    w.f32(0.25)
        .f32(-1.0)
        .i32(2)
        .i32(1)
        .i32(2)
        .u32(0)
        .u32(6)
        .u32(0)
        .u32(3);
    w.finish()
}

fn packed(at: usize, floats: bool) -> PackedBitVector {
    PackedBitVector {
        num_items: 1,
        float_range: floats.then_some([2.0, -1.0]),
        data: at..at + 1,
        bit_size: 8,
    }
}

/// Packed vectors start at byte 328 and take 24 bytes with floats, 16 without.
fn expected_compressed() -> CompressedMesh {
    let mut at = 328;
    let mut next = |floats: bool| {
        let vector = packed(at + if floats { 16 } else { 8 }, floats);
        at += if floats { 24 } else { 16 };
        vector
    };
    CompressedMesh {
        vertices: next(true),
        uv: next(true),
        normals: next(true),
        tangents: next(true),
        weights: next(false),
        normal_signs: next(false),
        tangent_signs: next(false),
        float_colors: next(true),
        bone_indices: next(false),
        triangles: next(false),
        uv_info: 9,
    }
}

fn expected_shapes() -> BlendShapeData {
    BlendShapeData {
        vertices: vec![BlendShapeVertex {
            vertex: [1.0, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            tangent: [0.0, 0.0, 1.0],
            index: 3,
        }],
        shapes: vec![BlendShape {
            first_vertex: 0,
            vertex_count: 1,
            has_normals: true,
            has_tangents: false,
        }],
        channels: vec![BlendShapeChannel {
            name: "Smile".into(),
            name_hash: 77,
            frame_index: 0,
            frame_count: 1,
        }],
        full_weights: vec![100.0],
    }
}

fn expected_skin() -> Skin {
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    Skin {
        bind_poses: vec![identity],
        bone_name_hashes: vec![11, 12],
        root_bone_name_hash: 13,
        bones_aabb: vec![[[-1.0; 3], [1.0; 3]]],
        variable_bone_count_weights: vec![4, 5],
    }
}

fn expected_mesh() -> Mesh {
    let aabb = Aabb {
        center: [1.0, 2.0, 3.0],
        extent: [0.5; 3],
    };
    let channel = |stream, offset, format, dimension| ChannelInfo {
        stream,
        offset,
        format,
        dimension,
    };
    Mesh {
        name: "Hex".into(),
        sub_meshes: vec![SubMesh {
            first_byte: 2,
            index_count: 6,
            topology: 1,
            base_vertex: 3,
            first_vertex: 5,
            vertex_count: 4,
            local_aabb: aabb,
        }],
        blend_shapes: expected_shapes(),
        skin: expected_skin(),
        compression: 2,
        is_readable: true,
        keep_vertices: false,
        keep_indices: true,
        index_format: 0,
        index_buffer: 292..298,
        vertex_data: VertexData {
            vertex_count: 4,
            channels: vec![channel(0, 0, 0, 3), channel(1, 12, 2, 3)],
            data: 320..325,
        },
        compressed: expected_compressed(),
        local_aabb: aabb,
        usage_flags: 1,
        collision: BakedCollision {
            cooking_options: 30,
            convex: 568..569,
            triangles: 576..576,
        },
        metrics: [1.0, 0.5],
        stream: StreamingInfo {
            offset: 512,
            size: 96,
            path: "resources.assets.resS".into(),
        },
        lod: MeshLodInfo {
            lod_slope: 0.25,
            lod_bias: -1.0,
            num_levels: 2,
            sub_meshes: vec![vec![[0, 6], [0, 3]]],
        },
    }
}

#[test]
fn decodes_every_part_of_a_mesh_in_both_byte_orders() {
    for big_endian in [false, true] {
        let decoded: Mesh = decode_one(ClassId::MESH, mesh_bytes(big_endian), big_endian).unwrap();
        assert_eq!(decoded, expected_mesh());
    }
}

#[test]
fn each_keep_flag_decodes_to_its_own_field() {
    for (set, flags) in [[0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]]
        .into_iter()
        .enumerate()
    {
        let mesh: Mesh = decode_one(ClassId::MESH, mesh_bytes_with(false, flags), false).unwrap();
        let decoded = [mesh.is_readable, mesh.keep_vertices, mesh.keep_indices];
        assert_eq!(
            decoded.map(usize::from),
            [0, 1, 2].map(|i| usize::from(i == set))
        );
    }
}

#[test]
fn every_truncated_mesh_fails() {
    let bytes = mesh_bytes(false);
    for end in 0..bytes.len() {
        let error = decode_one::<Mesh>(ClassId::MESH, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "prefix {end}: {error}"
        );
    }
}

#[test]
fn rejects_nested_counts_the_mesh_cannot_hold() {
    let mut bytes = mesh_bytes(false);
    // The last level count sits 4 + 2 * 8 bytes before the end.
    let at = bytes.len() - 20;
    bytes[at..at + 4].copy_from_slice(&1_000_i32.to_le_bytes());
    let error = decode_one::<Mesh>(ClassId::MESH, bytes, false).unwrap_err();
    assert!(
        matches!(
            error,
            SerializedError::InvalidCount {
                what: "lod level",
                ..
            }
        ),
        "{error}"
    );
}
