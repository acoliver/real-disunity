//! Serialized `Mesh` objects for tests: the fields in the order the player stores them.

use worldrunner_unity_serialized::write::ObjectWriter;

/// A vertex attribute channel as stored: stream, byte offset, format and dimension.
pub type Channel = [u8; 4];

/// One sub-mesh: first byte, index count, topology, base vertex, first vertex, vertex count.
pub type SubMeshSpec = [u32; 6];

pub struct MeshSpec {
    pub name: &'static str,
    pub sub_meshes: Vec<SubMeshSpec>,
    /// 0 for an uncompressed mesh.
    pub compression: u8,
    /// 0 for 16-bit indices, 1 for 32-bit.
    pub index_format: i32,
    pub indices: Vec<u8>,
    pub vertex_count: u32,
    /// The 14 channels in the player's fixed order; an unused channel has dimension 0.
    pub channels: Vec<Channel>,
    /// Empty when the vertex data is streamed.
    pub vertex_data: Vec<u8>,
    /// Offset, size and path of streamed vertex data; empty path when inline.
    pub stream: (u64, u32, &'static str),
}

const BOUNDS: [f32; 6] = [0.5, 0.5, 0.0, 0.5, 0.5, 0.0];

fn write_empty_packed(w: &mut ObjectWriter, floats: bool) {
    w.u32(0);
    if floats {
        w.floats(&[0.0, 0.0]);
    }
    w.byte_array(&[]).unwrap().u8(0).align();
}

fn write_compressed(w: &mut ObjectWriter) {
    for floats in [
        true, true, true, true, false, false, false, true, false, false,
    ] {
        write_empty_packed(w, floats);
    }
    w.u32(0);
}

pub fn mesh_bytes(spec: &MeshSpec) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(spec.name).unwrap();
    w.i32(i32::try_from(spec.sub_meshes.len()).unwrap());
    for sub in &spec.sub_meshes {
        let [first_byte, count, topology, base, first, vertices] = *sub;
        w.u32(first_byte).u32(count).u32(topology);
        w.u32(base).u32(first).u32(vertices).floats(&BOUNDS);
    }
    w.i32(0).i32(0).i32(0).i32(0);
    w.i32(0).i32(0).u32(0).i32(0).i32(0);
    w.bytes(&[spec.compression, 1, 0, 1]).align();
    w.i32(spec.index_format);
    w.byte_array(&spec.indices).unwrap();
    w.u32(spec.vertex_count);
    w.i32(i32::try_from(spec.channels.len()).unwrap());
    for channel in &spec.channels {
        w.bytes(channel);
    }
    w.byte_array(&spec.vertex_data).unwrap().align();
    write_compressed(&mut w);
    w.floats(&BOUNDS).i32(0).i32(0);
    w.byte_array(&[]).unwrap().byte_array(&[]).unwrap();
    w.floats(&[0.0, 0.0]);
    let (offset, size, path) = spec.stream;
    w.u64(offset).u32(size).string(path).unwrap();
    w.f32(0.0).f32(0.0).i32(0).i32(0);
    w.finish()
}
