//! Stored meshes and textures as render-program's meshes and source textures.

use worldrunner_disunity_runtime::{MeshRun, PlayerMesh, PlayerTexture};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::offscreen::frame::{ChannelFormat, MeshIndices};

use super::super::assets::{
    COLOR, MeshError, NORMAL, POSITION, TEXTURE_COORDINATES, mesh_of, texture_of,
};

fn mesh() -> PlayerMesh {
    PlayerMesh {
        generation: 4,
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: Vec::new(),
        uvs: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        colors: Vec::new(),
        indices: vec![0, 1, 2, 2, 1, 0],
        submeshes: vec![MeshRun {
            slot: 0,
            first_index: 0,
            index_count: 6,
        }],
        left_out: 0,
    }
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

#[test]
fn a_mesh_keeps_positions_texture_coordinates_and_indices_in_the_channels_shaders_read() {
    let made = mesh_of(&mesh()).unwrap();
    assert_eq!(made.vertex_count, 3);
    let sources: Vec<i8> = made.channels.iter().map(|channel| channel.source).collect();
    assert_eq!(sources, [POSITION, TEXTURE_COORDINATES]);
    assert_eq!(made.channels[0].format, ChannelFormat::Float32x3);
    assert_eq!(
        floats(&made.channels[0].bytes),
        [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
    );
    assert_eq!(made.channels[1].format, ChannelFormat::Float32x2);
    assert_eq!(
        floats(&made.channels[1].bytes),
        [0.0, 0.0, 1.0, 0.0, 0.0, 1.0]
    );
    let MeshIndices::U32(indices) = &made.indices else {
        panic!("32-bit indices");
    };
    assert_eq!(indices, &[0, 1, 2, 2, 1, 0]);
}

#[test]
fn normals_and_colors_are_channels_only_when_the_mesh_has_them() {
    let mut stored = mesh();
    stored.normals = vec![[0.0, 0.0, 1.0]; 3];
    stored.colors = vec![[1.0, 0.5, 0.25, 1.0]; 3];
    let made = mesh_of(&stored).unwrap();
    let sources: Vec<i8> = made.channels.iter().map(|channel| channel.source).collect();
    assert_eq!(sources, [POSITION, NORMAL, COLOR, TEXTURE_COORDINATES]);
    assert_eq!(made.channels[2].format, ChannelFormat::Float32x4);
    assert_eq!(floats(&made.channels[2].bytes)[..4], [1.0, 0.5, 0.25, 1.0]);
}

#[test]
fn sub_meshes_keep_the_slot_the_managed_mesh_numbered_them_with_gaps_that_draw_nothing() {
    let mut stored = mesh();
    stored.submeshes = vec![
        MeshRun {
            slot: 0,
            first_index: 0,
            index_count: 3,
        },
        MeshRun {
            slot: 2,
            first_index: 3,
            index_count: 3,
        },
    ];
    let made = mesh_of(&stored).unwrap();
    let ranges: Vec<_> = made
        .sub_meshes
        .iter()
        .map(|sub| (sub.first, sub.count))
        .collect();
    assert_eq!(ranges, [(0, 3), (0, 0), (3, 3)]);
}

#[test]
fn a_mesh_that_cannot_be_drawn_is_named() {
    let mut empty = mesh();
    empty.indices.clear();
    assert_eq!(mesh_of(&empty).unwrap_err(), MeshError::NoTriangles);
    let mut short = mesh();
    short.uvs.pop();
    assert_eq!(
        mesh_of(&short).unwrap_err(),
        MeshError::ChannelLength {
            what: "texture coordinates",
            found: 2,
            expected: 3
        }
    );
    let mut far = mesh();
    far.submeshes[0].slot = 10_000;
    assert_eq!(mesh_of(&far).unwrap_err(), MeshError::SubMeshSlot(10_000));
}

fn texture(format: u32, linear: bool, bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        format,
        width: 2,
        height: 1,
        mips: 1,
        linear,
        generation: 1,
        distance_spread: 0,
        bytes,
    }
}

#[test]
fn a_color_texture_is_srgb_rgba8_and_a_data_texture_is_linear_with_the_same_texels() {
    let texels = vec![10, 20, 30, 40, 50, 60, 70, 80];
    let color = texture_of(TextureId(9), &texture(4, false, texels.clone())).unwrap();
    assert_eq!(
        (color.id, color.size, color.graphics_format),
        (TextureId(9), [2, 1], 4)
    );
    assert_eq!(color.bytes, texels);
    let data = texture_of(TextureId(9), &texture(4, true, texels.clone())).unwrap();
    assert_eq!(data.graphics_format, 8);
    assert_eq!(data.bytes, texels);
    assert!(color.sampler.is_none());
}

#[test]
fn rows_stay_bottom_first_as_the_engine_stores_them() {
    let mut stored = texture(
        4,
        false,
        vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4],
    );
    stored.height = 2;
    let made = texture_of(TextureId(1), &stored).unwrap();
    assert_eq!(
        made.bytes[..8],
        [1, 1, 1, 1, 2, 2, 2, 2],
        "row 0 is the engine's first row"
    );
}

#[test]
fn rgb24_gains_an_opaque_alpha_and_alpha8_becomes_white_with_that_alpha() {
    let rgb = texture_of(TextureId(1), &texture(3, false, vec![1, 2, 3, 4, 5, 6])).unwrap();
    assert_eq!(rgb.bytes, [1, 2, 3, 255, 4, 5, 6, 255]);
    let alpha = texture_of(TextureId(1), &texture(1, true, vec![7, 9])).unwrap();
    assert_eq!(alpha.bytes, [255, 255, 255, 7, 255, 255, 255, 9]);
}

#[test]
fn a_texture_in_a_format_the_host_cannot_read_or_cut_short_is_an_error() {
    assert!(texture_of(TextureId(1), &texture(27, false, vec![0; 16])).is_err());
    assert!(texture_of(TextureId(1), &texture(4, false, vec![0; 7])).is_err());
}
