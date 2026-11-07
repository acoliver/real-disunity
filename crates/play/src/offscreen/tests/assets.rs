//! Stored meshes and textures as render-program's meshes and source textures.

use worldrunner_disunity_runtime::{
    MeshChannel as StoredChannel, MeshChannels, MeshRange, PlayerTexture, TextureSampler,
};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::sampler::{Filter, SamplerState, Wrap};
use worldrunner_render_program::unity::offscreen::frame::{ChannelFormat, MeshIndices};
use worldrunner_world_simulation::graphics::WorldSampler;

use super::super::assets::{MeshError, mesh_of, sampler_of_stored, sampler_of_world, texture_of};

fn channel(number: u32, dimension: u32, data: Vec<f32>) -> StoredChannel {
    StoredChannel {
        number,
        dimension,
        data,
    }
}

fn triangles(start: u32, count: u32) -> MeshRange {
    MeshRange {
        start,
        count,
        base_vertex: 0,
        topology: 0,
    }
}

fn mesh() -> MeshChannels {
    MeshChannels {
        generation: 4,
        vertex_count: 3,
        channels: vec![
            channel(0, 3, vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]),
            channel(4, 2, vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0]),
        ],
        indices: vec![0, 1, 2, 2, 1, 0],
        ranges: vec![triangles(0, 6)],
    }
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn sources(made: &worldrunner_render_program::unity::offscreen::frame::Mesh) -> Vec<i8> {
    made.channels.iter().map(|channel| channel.source).collect()
}

#[test]
fn channels_keep_the_number_the_managed_mesh_stored_them_under_and_the_floats_it_gave() {
    let made = mesh_of(&mesh()).unwrap();
    assert_eq!(made.vertex_count, 3);
    assert_eq!(sources(&made), [0, 4]);
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
fn normals_tangents_colours_and_every_texture_coordinate_set_are_channels_by_their_number() {
    let mut stored = mesh();
    stored.channels = vec![
        channel(0, 3, vec![0.0; 9]),
        channel(1, 3, vec![0.5; 9]),
        channel(2, 4, vec![0.25; 12]),
        channel(
            3,
            4,
            vec![
                1.0, 0.5, 0.25, 1.0, 1.0, 0.5, 0.25, 1.0, 1.0, 0.5, 0.25, 1.0,
            ],
        ),
        channel(4, 2, vec![0.0; 6]),
        channel(5, 3, vec![0.0; 9]),
        channel(11, 1, vec![7.0, 8.0, 9.0]),
    ];
    let made = mesh_of(&stored).unwrap();
    assert_eq!(sources(&made), [0, 1, 2, 3, 4, 5, 11]);
    let formats: Vec<ChannelFormat> = made.channels.iter().map(|c| c.format).collect();
    assert_eq!(
        formats,
        [
            ChannelFormat::Float32x3,
            ChannelFormat::Float32x3,
            ChannelFormat::Float32x4,
            ChannelFormat::Float32x4,
            ChannelFormat::Float32x2,
            ChannelFormat::Float32x3,
            ChannelFormat::Float32,
        ]
    );
    assert_eq!(floats(&made.channels[6].bytes), [7.0, 8.0, 9.0]);
}

#[test]
fn blend_weights_and_indices_are_not_vertex_inputs_of_this_renderer() {
    let mut stored = mesh();
    stored.channels.push(channel(12, 4, vec![0.0; 12]));
    stored.channels.push(channel(13, 4, vec![0.0; 12]));
    assert_eq!(sources(&mesh_of(&stored).unwrap()), [0, 4]);
}

#[test]
fn sub_meshes_follow_the_range_number_with_empty_ranges_that_draw_nothing() {
    let mut stored = mesh();
    stored.ranges = vec![triangles(0, 3), triangles(3, 0), triangles(3, 3)];
    let made = mesh_of(&stored).unwrap();
    let ranges: Vec<_> = made
        .sub_meshes
        .iter()
        .map(|sub| (sub.first, sub.count, sub.base_vertex))
        .collect();
    assert_eq!(ranges, [(0, 3, 0), (3, 0, 0), (3, 3, 0)]);
}

#[test]
fn a_range_of_another_topology_is_refused_by_the_name_of_the_topology() {
    for (topology, expected) in [
        (1, MeshError::QuadTopology),
        (2, MeshError::LineTopology),
        (3, MeshError::LineStripTopology),
        (4, MeshError::PointTopology),
        (9, MeshError::UnknownTopology(9)),
    ] {
        let mut stored = mesh();
        stored.ranges = vec![
            triangles(0, 3),
            MeshRange {
                topology,
                ..triangles(3, 3)
            },
        ];
        assert_eq!(mesh_of(&stored).unwrap_err(), expected);
    }
    // An empty range draws nothing, so its topology does not matter.
    let mut empty = mesh();
    empty.ranges.push(MeshRange {
        topology: 2,
        ..triangles(6, 0)
    });
    assert!(mesh_of(&empty).is_ok());
}

#[test]
fn a_mesh_that_cannot_be_drawn_is_named() {
    let mut empty = mesh();
    empty.indices.clear();
    empty.ranges = vec![triangles(0, 0)];
    assert_eq!(mesh_of(&empty).unwrap_err(), MeshError::NoTriangles);
    let mut no_positions = mesh();
    no_positions.channels.remove(0);
    assert_eq!(mesh_of(&no_positions).unwrap_err(), MeshError::NoPositions);
    let mut short = mesh();
    short.channels[1].data.pop();
    assert_eq!(
        mesh_of(&short).unwrap_err(),
        MeshError::ChannelLength {
            number: 4,
            found: 5,
            expected: 6
        }
    );
    let mut wide = mesh();
    wide.channels[1].dimension = 5;
    assert_eq!(
        mesh_of(&wide).unwrap_err(),
        MeshError::Dimension {
            number: 4,
            dimension: 5
        }
    );
    let mut far = mesh();
    far.ranges = vec![triangles(0, 3); 10_000];
    assert_eq!(mesh_of(&far).unwrap_err(), MeshError::SubMeshSlot(10_000));
}

fn stored_sampler(wrap: [u32; 3], filter: u32) -> TextureSampler {
    TextureSampler {
        generation: 1,
        wrap_u: wrap[0],
        wrap_v: wrap[1],
        wrap_w: wrap[2],
        filter,
        aniso_level: 8,
        mip_bias: 0.5,
    }
}

#[test]
fn a_stored_textures_wrap_and_filter_become_the_sampler_state() {
    let state = sampler_of_stored(&stored_sampler([1, 2, 0], 0));
    assert_eq!(
        state,
        SamplerState {
            wrap_u: Wrap::Clamp,
            wrap_v: Wrap::Mirror,
            filter: Filter::Point,
        }
    );
    assert_eq!(
        sampler_of_stored(&stored_sampler([0, 0, 0], 1)),
        SamplerState {
            wrap_u: Wrap::Repeat,
            wrap_v: Wrap::Repeat,
            filter: Filter::Bilinear,
        }
    );
    assert_eq!(
        sampler_of_stored(&stored_sampler([0, 0, 0], 2)).filter,
        Filter::Trilinear
    );
}

#[test]
fn mirror_once_is_read_as_mirror_and_unknown_values_as_the_engines_defaults() {
    assert_eq!(
        sampler_of_stored(&stored_sampler([3, 3, 3], 1)).wrap_u,
        Wrap::Mirror
    );
    let odd = sampler_of_stored(&stored_sampler([99, 99, 0], 99));
    assert_eq!(
        (odd.wrap_u, odd.wrap_v, odd.filter),
        (Wrap::Repeat, Wrap::Repeat, Filter::Bilinear)
    );
}

#[test]
fn a_render_textures_sampler_maps_like_a_stored_textures() {
    let world = WorldSampler {
        wrap_u: 1,
        wrap_v: 0,
        wrap_w: 0,
        filter: 2,
        aniso_level: 1,
        mip_bias: 0.0,
    };
    assert_eq!(
        sampler_of_world(&world),
        SamplerState {
            wrap_u: Wrap::Clamp,
            wrap_v: Wrap::Repeat,
            filter: Filter::Trilinear,
        }
    );
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
    let color = texture_of(TextureId(9), &texture(4, false, texels.clone()), None).unwrap();
    assert_eq!(
        (color.id, color.size, color.graphics_format),
        (TextureId(9), [2, 1], 4)
    );
    assert_eq!(color.bytes, texels);
    let data = texture_of(TextureId(9), &texture(4, true, texels.clone()), None).unwrap();
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
    let made = texture_of(TextureId(1), &stored, None).unwrap();
    assert_eq!(
        made.bytes[..8],
        [1, 1, 1, 1, 2, 2, 2, 2],
        "row 0 is the engine's first row"
    );
}

#[test]
fn rgb24_gains_an_opaque_alpha_and_alpha8_becomes_white_with_that_alpha() {
    let rgb = texture_of(
        TextureId(1),
        &texture(3, false, vec![1, 2, 3, 4, 5, 6]),
        None,
    )
    .unwrap();
    assert_eq!(rgb.bytes, [1, 2, 3, 255, 4, 5, 6, 255]);
    let alpha = texture_of(TextureId(1), &texture(1, true, vec![7, 9]), None).unwrap();
    assert_eq!(alpha.bytes, [255, 255, 255, 7, 255, 255, 255, 9]);
}

#[test]
fn a_texture_in_a_format_the_host_cannot_read_or_cut_short_is_an_error() {
    assert!(texture_of(TextureId(1), &texture(27, false, vec![0; 16]), None).is_err());
    assert!(texture_of(TextureId(1), &texture(4, false, vec![0; 7]), None).is_err());
}

#[test]
fn a_stored_texture_is_read_with_the_sampler_it_is_given() {
    let state = sampler_of_stored(&stored_sampler([1, 1, 0], 0));
    let made = texture_of(TextureId(1), &texture(4, false, vec![0; 8]), Some(state)).unwrap();
    assert_eq!(made.sampler, Some(state));
}

#[test]
fn a_bc6h_texture_keeps_the_blocks_of_its_base_level_as_unsigned_float_blocks() {
    // 8x4 texels are two 16-byte blocks; the stored bytes go on with the one block of the next level.
    let bytes: Vec<u8> = (0..48).collect();
    let mut stored = texture(24, true, bytes.clone());
    (stored.width, stored.height, stored.mips) = (8, 4, 2);
    let made = texture_of(TextureId(3), &stored, None).unwrap();
    assert_eq!((made.size, made.graphics_format), ([8, 4], 106));
    assert_eq!(made.bytes, bytes[..32]);
    assert!(made.sampler.is_none());
}

#[test]
fn a_bc6h_texture_cut_short_of_its_base_level_is_an_error() {
    let mut stored = texture(24, true, vec![0; 31]);
    (stored.width, stored.height) = (8, 4);
    let error = texture_of(TextureId(3), &stored, None).unwrap_err();
    assert!(
        matches!(
            error,
            crate::textures::PlayerTextureError::Short {
                needed: 32,
                actual: 31,
                ..
            }
        ),
        "{error}"
    );
}
