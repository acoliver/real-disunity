//! Stored meshes and textures as render-program's meshes and source textures.

use worldrunner_disunity_runtime::{
    MeshChannel as StoredChannel, MeshChannels, MeshRange, PlayerTexture, TextureDimension,
    TextureSampler,
};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::sampler::{Filter, SamplerState, Wrap};
use worldrunner_render_program::unity::offscreen::frame::{
    ChannelFormat, MeshIndices, SourceDimension,
};
use worldrunner_world_simulation::graphics::WorldSampler;

use super::super::assets::{
    Emptiness, MeshError, mesh_of, sampler_of_stored, sampler_of_world, texture_of,
};

pub(super) fn channel(number: u32, dimension: u32, data: Vec<f32>) -> StoredChannel {
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

pub(super) fn mesh() -> MeshChannels {
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

pub(super) fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

pub(super) fn sources(made: &worldrunner_render_program::unity::offscreen::frame::Mesh) -> Vec<i8> {
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

fn empty_of(stored: &MeshChannels) -> Emptiness {
    match mesh_of(stored).unwrap_err() {
        MeshError::Empty(empty) => empty,
        other => panic!("an empty mesh, not {other:?}"),
    }
}

#[test]
fn a_mesh_with_nothing_to_draw_is_empty_by_what_it_holds() {
    // Vertices and one sub-mesh record with no indices.
    let mut no_indices = mesh();
    no_indices.indices.clear();
    no_indices.ranges = vec![triangles(0, 0)];
    assert_eq!(
        empty_of(&no_indices),
        Emptiness {
            vertices: true,
            indices: false,
            ranges: 1,
            topologies: vec![0],
        }
    );
    // Vertices alone: no indices and no sub-mesh records.
    let mut bare = mesh();
    bare.indices.clear();
    bare.ranges.clear();
    assert_eq!(
        empty_of(&bare),
        Emptiness {
            vertices: true,
            indices: false,
            ranges: 0,
            topologies: Vec::new(),
        }
    );
    // Indices that no sub-mesh reads: two empty records, one of lines.
    let mut unread = mesh();
    unread.ranges = vec![
        triangles(0, 0),
        MeshRange {
            topology: 2,
            ..triangles(0, 0)
        },
    ];
    assert_eq!(
        empty_of(&unread),
        Emptiness {
            vertices: true,
            indices: true,
            ranges: 2,
            topologies: vec![0, 2],
        }
    );
    // No vertices at all.
    let mut none = mesh();
    none.vertex_count = 0;
    none.channels.clear();
    none.indices.clear();
    none.ranges.clear();
    assert_eq!(
        empty_of(&none),
        Emptiness {
            vertices: false,
            indices: false,
            ranges: 0,
            topologies: Vec::new(),
        }
    );
}

#[test]
fn an_emptiness_is_described_without_counts_that_differ_between_meshes() {
    let empty = Emptiness {
        vertices: true,
        indices: false,
        ranges: 2,
        topologies: vec![0, 4],
    };
    assert_eq!(
        empty.to_string(),
        "2 sub-meshes with topologies [0, 4], no indices, with vertices"
    );
}

#[test]
fn a_sub_mesh_that_draws_from_indices_the_mesh_lacks_is_refused_by_name() {
    let mut lacking = mesh();
    lacking.indices.clear();
    lacking.ranges = vec![triangles(0, 3)];
    assert_eq!(mesh_of(&lacking).unwrap_err(), MeshError::IndicesMissing);
}

#[test]
fn a_mesh_that_cannot_be_drawn_is_named() {
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
    sampler_with_aniso(wrap, filter, 8)
}

fn sampler_with_aniso(wrap: [u32; 3], filter: u32, aniso_level: u32) -> TextureSampler {
    TextureSampler {
        generation: 1,
        wrap_u: wrap[0],
        wrap_v: wrap[1],
        wrap_w: wrap[2],
        filter,
        aniso_level,
        mip_bias: 0.5,
    }
}

fn world_sampler(wrap: [u32; 3], filter: u32, aniso_level: u32) -> WorldSampler {
    WorldSampler {
        wrap_u: wrap[0],
        wrap_v: wrap[1],
        wrap_w: wrap[2],
        filter,
        aniso_level,
        mip_bias: 0.5,
    }
}

#[test]
fn a_stored_textures_wrap_filter_and_anisotropy_become_the_sampler_state() {
    let state = sampler_of_stored(&stored_sampler([1, 2, 0], 0));
    assert_eq!(
        state,
        SamplerState {
            wrap_u: Wrap::Clamp,
            wrap_v: Wrap::Mirror,
            wrap_w: Wrap::Repeat,
            filter: Filter::Point,
            anisotropy: 8,
        }
    );
    assert_eq!(
        sampler_of_stored(&sampler_with_aniso([0, 0, 0], 1, 1)),
        SamplerState {
            wrap_u: Wrap::Repeat,
            wrap_v: Wrap::Repeat,
            wrap_w: Wrap::Repeat,
            filter: Filter::Bilinear,
            anisotropy: 1,
        }
    );
    assert_eq!(
        sampler_of_stored(&stored_sampler([0, 0, 0], 2)).filter,
        Filter::Trilinear
    );
}

#[test]
fn the_wrap_along_w_of_a_stored_texture_maps_like_the_other_axes() {
    let along_w = |mode| sampler_of_stored(&stored_sampler([0, 0, mode], 1)).wrap_w;
    assert_eq!(along_w(0), Wrap::Repeat);
    assert_eq!(along_w(1), Wrap::Clamp);
    assert_eq!(along_w(2), Wrap::Mirror);
    assert_eq!(along_w(3), Wrap::Mirror);
    assert_eq!(along_w(99), Wrap::Repeat);
}

#[test]
fn an_anisotropy_level_is_carried_and_zero_reads_as_one_and_more_than_sixteen_as_sixteen() {
    let level = |level| sampler_of_stored(&sampler_with_aniso([0; 3], 2, level)).anisotropy;
    let world_level = |level| sampler_of_world(&world_sampler([0; 3], 2, level)).anisotropy;
    for (given, read) in [
        (0, 1),
        (1, 1),
        (2, 2),
        (8, 8),
        (16, 16),
        (17, 16),
        (u32::MAX, 16),
    ] {
        assert_eq!(level(given), read, "stored level {given}");
        assert_eq!(world_level(given), read, "render texture level {given}");
    }
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
    let world = world_sampler([1, 0, 2], 2, 4);
    assert_eq!(
        sampler_of_world(&world),
        SamplerState {
            wrap_u: Wrap::Clamp,
            wrap_v: Wrap::Repeat,
            wrap_w: Wrap::Mirror,
            filter: Filter::Trilinear,
            anisotropy: 4,
        }
    );
}

pub(super) fn texture(format: u32, linear: bool, bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        format,
        width: 2,
        height: 1,
        mips: 1,
        linear,
        generation: 1,
        distance_spread: 0,
        depth: 0,
        dimension: worldrunner_disunity_runtime::TextureDimension::D2,
        graphics_format: 0,
        bytes,
    }
}

#[test]
fn a_color_texture_is_srgb_rgba8_and_a_data_texture_is_linear_with_the_same_texels() {
    let texels = vec![10, 20, 30, 40, 50, 60, 70, 80];
    let color = texture_of(
        TextureId(9),
        &texture(4, false, texels.clone()),
        None,
        false,
    )
    .unwrap();
    assert_eq!(
        (color.id, color.size, color.graphics_format),
        (TextureId(9), [2, 1], 4)
    );
    assert_eq!(color.bytes, texels);
    let data = texture_of(TextureId(9), &texture(4, true, texels.clone()), None, false).unwrap();
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
    let made = texture_of(TextureId(1), &stored, None, false).unwrap();
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
        false,
    )
    .unwrap();
    assert_eq!(rgb.bytes, [1, 2, 3, 255, 4, 5, 6, 255]);
    let alpha = texture_of(TextureId(1), &texture(1, true, vec![7, 9]), None, false).unwrap();
    assert_eq!(alpha.bytes, [255, 255, 255, 7, 255, 255, 255, 9]);
}

#[test]
fn a_texture_in_a_format_the_host_cannot_read_or_cut_short_is_an_error() {
    assert!(texture_of(TextureId(1), &texture(27, false, vec![0; 16]), None, false).is_err());
    assert!(texture_of(TextureId(1), &texture(4, false, vec![0; 7]), None, false).is_err());
}

/// A stored array of `depth` layers of 2x1 texels whose bytes are `bytes`.
fn array(format: u32, depth: u32, bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        depth,
        dimension: TextureDimension::D2Array,
        graphics_format: 8,
        ..texture(format, false, bytes)
    }
}

#[test]
fn a_stored_array_is_one_source_of_its_layers_in_the_order_the_engine_stored_them() {
    let stored = array(3, 3, (1..=18).collect());
    let made = texture_of(TextureId(9), &stored, None, true).unwrap();
    assert_eq!(
        (made.id, made.dimension, made.size, made.graphics_format),
        (TextureId(9), SourceDimension::D2Array(3), [2, 1], 4)
    );
    let layers = [
        [1, 2, 3, 255, 4, 5, 6, 255],
        [7, 8, 9, 255, 10, 11, 12, 255],
        [13, 14, 15, 255, 16, 17, 18, 255],
    ];
    assert_eq!(made.bytes, layers.concat());
    let data = PlayerTexture {
        linear: true,
        ..stored
    };
    assert_eq!(
        texture_of(TextureId(9), &data, None, true)
            .unwrap()
            .graphics_format,
        8
    );
}

#[test]
fn a_texture_of_the_2d_dimension_is_plain_whatever_layer_count_it_states() {
    for depth in [0, 1] {
        let stored = PlayerTexture {
            dimension: TextureDimension::D2,
            ..array(4, depth, vec![5; 8])
        };
        let made = texture_of(TextureId(1), &stored, None, false).unwrap();
        assert_eq!(made.dimension, SourceDimension::D2, "depth {depth}");
        assert_eq!(made.bytes, [5; 8]);
    }
}

#[test]
fn a_one_layer_array_stays_an_array() {
    let made = texture_of(TextureId(1), &array(4, 1, vec![5; 8]), None, false).unwrap();
    assert_eq!(made.dimension, SourceDimension::D2Array(1));
    assert_eq!(made.bytes, [5; 8]);
}

#[test]
fn an_array_whose_layers_hold_a_mip_chain_keeps_every_level_of_each_layer() {
    // Two layers of 2x1 RGBA32 texels, each followed by its 1x1 second level.
    let layer = |texel: u8| [vec![texel; 8], vec![texel + 10; 4]].concat();
    let stored = PlayerTexture {
        mips: 2,
        ..array(4, 2, [layer(1), layer(2)].concat())
    };
    let made = texture_of(TextureId(1), &stored, None, false).unwrap();
    assert_eq!(made.dimension, SourceDimension::D2Array(2));
    assert_eq!(made.mips, 2);
    assert_eq!(made.bytes, [layer(1), layer(2)].concat());
}

#[test]
fn every_level_of_a_narrower_format_is_read_as_rgba8_with_its_own_size() {
    // RGB24 layers of 2x1 texels (6 bytes) and a 1x1 second level (3 bytes), then a second layer.
    let stored = PlayerTexture {
        mips: 2,
        ..array(3, 2, (1..=18).collect())
    };
    let made = texture_of(TextureId(1), &stored, None, false).unwrap();
    let expected = [
        [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255],
        [10, 11, 12, 255, 13, 14, 15, 255, 16, 17, 18, 255],
    ];
    assert_eq!(made.bytes, expected.concat());
    assert_eq!(made.mips, 2);
}

#[test]
fn an_array_with_more_levels_than_its_size_has_is_an_error_by_name() {
    use crate::textures::PlayerTextureError::Mips;
    // A 2x1 texture has two levels, 2x1 and 1x1.
    for mips in [3, u32::MAX] {
        let stored = PlayerTexture {
            mips,
            ..array(4, 2, vec![0; 64])
        };
        let error = texture_of(TextureId(1), &stored, None, false).unwrap_err();
        assert_eq!(
            error,
            Mips {
                mips,
                width: 2,
                height: 1
            }
        );
    }
}

#[test]
fn an_array_whose_layers_are_longer_than_their_chain_is_an_error_by_name() {
    use crate::textures::PlayerTextureError::Levels;
    // The chain of 2x1 RGBA32 texels with two levels is 12 bytes; each layer holds 16.
    let stored = PlayerTexture {
        mips: 2,
        ..array(4, 2, vec![0; 32])
    };
    let error = texture_of(TextureId(1), &stored, None, false).unwrap_err();
    assert_eq!(
        error,
        Levels {
            mips: 2,
            needed: 12,
            actual: 16
        }
    );
}

#[test]
fn an_array_that_is_not_equal_layers_of_a_format_the_host_reads_is_an_error_by_name() {
    use crate::textures::PlayerTextureError::{Layers, Short, Unsupported};
    let id = TextureId(1);
    // 7 bytes are not two layers, nor are none.
    let uneven = texture_of(id, &array(4, 2, vec![0; 7]), None, false).unwrap_err();
    assert_eq!(
        uneven,
        Layers {
            depth: 2,
            actual: 7
        }
    );
    let empty = texture_of(id, &array(4, 2, Vec::new()), None, false).unwrap_err();
    assert_eq!(
        empty,
        Layers {
            depth: 2,
            actual: 0
        }
    );
    // Two layers of 7 bytes are each short of the 8 a 2x1 RGBA32 level needs.
    let short = texture_of(id, &array(4, 2, vec![0; 14]), None, false).unwrap_err();
    assert!(
        matches!(
            short,
            Short {
                needed: 8,
                actual: 7,
                ..
            }
        ),
        "{short}"
    );
    // Formats the host has no layout for are refused by name, as BC7 arrays are.
    let bc7 = texture_of(id, &array(25, 2, vec![0; 32]), None, true).unwrap_err();
    assert_eq!(
        bc7,
        Unsupported {
            format: "BC7 array".into()
        }
    );
    // A format only the GraphicsFormat names is refused by that name.
    let graphics = PlayerTexture {
        format: 0,
        graphics_format: 52,
        ..array(0, 2, vec![0; 32])
    };
    let float = texture_of(id, &graphics, None, true).unwrap_err();
    assert_eq!(
        float,
        Unsupported {
            format: "GraphicsFormat 52 array".into()
        }
    );
}

#[test]
fn a_stored_texture_is_read_with_the_sampler_it_is_given() {
    let state = sampler_of_stored(&stored_sampler([1, 1, 0], 0));
    let made = texture_of(
        TextureId(1),
        &texture(4, false, vec![0; 8]),
        Some(state),
        false,
    )
    .unwrap();
    assert_eq!(made.sampler, Some(state));
}

#[test]
fn a_bc6h_texture_keeps_the_blocks_of_its_base_level_as_unsigned_float_blocks() {
    // 8x4 texels are two 16-byte blocks; the stored bytes go on with the one block of the next level.
    let bytes: Vec<u8> = (0..48).collect();
    let mut stored = texture(24, true, bytes.clone());
    (stored.width, stored.height, stored.mips) = (8, 4, 2);
    let made = texture_of(TextureId(3), &stored, None, false).unwrap();
    assert_eq!((made.size, made.graphics_format), ([8, 4], 106));
    assert_eq!(made.bytes, bytes[..32]);
    assert!(made.sampler.is_none());
}

#[test]
fn a_bc6h_texture_cut_short_of_its_base_level_is_an_error() {
    let mut stored = texture(24, true, vec![0; 31]);
    (stored.width, stored.height) = (8, 4);
    let error = texture_of(TextureId(3), &stored, None, false).unwrap_err();
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

/// A stored block texture of `width` by `height` texels with `bytes`.
fn blocks_of(
    format: u32,
    linear: bool,
    (width, height): (u32, u32),
    bytes: Vec<u8>,
) -> PlayerTexture {
    PlayerTexture {
        width,
        height,
        ..texture(format, linear, bytes)
    }
}

#[test]
fn dxt1_dxt5_bc4_and_bc7_keep_their_blocks_in_the_colour_space_they_are_stored_in() {
    // Each format with the bytes of one block, then of a second (the next level, which is not
    // carried): the first block goes up as it is.
    let cases = [
        (10, 8, [(false, 96), (true, 97)]),
        (12, 16, [(false, 100), (true, 101)]),
        (25, 16, [(false, 108), (true, 109)]),
        // BC4 has no sRGB form: a data texture goes up as blocks, a colour one is decoded.
        (26, 8, [(false, 4), (true, 102)]),
    ];
    for (format, block, spaces) in cases {
        for (linear, code) in spaces {
            let bytes: Vec<u8> = (0..u8::try_from(block * 2).unwrap()).collect();
            let stored = blocks_of(format, linear, (4, 4), bytes.clone());
            let made = texture_of(TextureId(1), &stored, None, true).unwrap();
            assert_eq!(
                (made.size, made.graphics_format),
                ([4, 4], code),
                "format {format} linear {linear}"
            );
            if code == 4 {
                assert_eq!(made.bytes.len(), 4 * 4 * 4, "format {format} decoded");
            } else {
                assert_eq!(made.bytes, bytes[..block], "format {format}");
            }
        }
    }
}

#[test]
fn a_block_texture_is_decoded_when_blocks_are_not_wanted_or_its_sides_are_not_whole_blocks() {
    let bytes = vec![0; 16 * 4];
    let stored = blocks_of(12, false, (4, 4), bytes.clone());
    let decoded = texture_of(TextureId(1), &stored, None, false).unwrap();
    assert_eq!(
        (decoded.graphics_format, decoded.bytes.len()),
        (4, 4 * 4 * 4)
    );
    // A 6x4 texture has two blocks across but is not a multiple of four wide.
    let stored = blocks_of(12, false, (6, 4), bytes);
    let decoded = texture_of(TextureId(1), &stored, None, true).unwrap();
    assert_eq!(
        (decoded.graphics_format, decoded.bytes.len()),
        (4, 6 * 4 * 4)
    );
}

#[test]
fn a_block_texture_cut_short_of_its_base_level_is_an_error_by_its_name() {
    let stored = blocks_of(25, false, (8, 4), vec![0; 31]);
    let error = texture_of(TextureId(1), &stored, None, true).unwrap_err();
    assert!(
        matches!(
            error,
            crate::textures::PlayerTextureError::Short {
                format: "BC7",
                needed: 32,
                actual: 31,
                ..
            }
        ),
        "{error}"
    );
}
