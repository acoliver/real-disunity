use worldrunner_disunity_runtime::PlayerTexture;
use worldrunner_render_plan::{SceneTexture, TextureColorSpace};
use worldrunner_render_scene::texture::prepare_texture;

use crate::textures::{BlockPath, PlayerTextureError};

fn player_scene_texture(texture: &PlayerTexture) -> Result<SceneTexture, PlayerTextureError> {
    crate::textures::player_scene_texture(texture, BlockPath::Dds)
}

fn decoded(texture: &PlayerTexture) -> Result<SceneTexture, PlayerTextureError> {
    crate::textures::player_scene_texture(texture, BlockPath::Rgba8)
}

const ALPHA8: u32 = 1;
const RGB24: u32 = 3;
const RGBA32: u32 = 4;
const DXT1: u32 = 10;
const DXT5: u32 = 12;
const BC4: u32 = 26;

fn player(format: u32, size: [u32; 2], linear: bool, bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        format,
        width: size[0],
        height: size[1],
        mips: 1,
        linear,
        generation: 1,
        distance_spread: 0,
        bytes,
    }
}

/// An Alpha8 atlas holding distance fields of the given spread, one row of `bytes`.
fn field(spread: u32, bytes: Vec<u8>) -> PlayerTexture {
    PlayerTexture {
        distance_spread: spread,
        ..player(ALPHA8, [bytes.len() as u32, 1], true, bytes)
    }
}

/// The coverage a distance field's bytes become, one alpha per input byte.
fn coverage(spread: u32, field_bytes: &[u8]) -> Vec<u8> {
    let scene = player_scene_texture(&field(spread, field_bytes.to_vec())).unwrap();
    let (_, rows) = texels(&scene);
    rows.chunks_exact(4)
        .map(|texel| {
            assert_eq!(
                texel[..3],
                [255, 255, 255],
                "coverage is white with an alpha"
            );
            texel[3]
        })
        .collect()
}

fn texels(texture: &SceneTexture) -> (TextureColorSpace, &[u8]) {
    match texture {
        SceneTexture::Texels {
            color_space,
            bytes,
            mips: 1,
            ..
        } => (*color_space, bytes),
        other => panic!("expected one-level texels, got {other:?}"),
    }
}

/// The renderer's own view of a container: its GPU format name, size, and base level bytes.
fn prepared(texture: &SceneTexture) -> (String, [u32; 2], Vec<u8>) {
    let prepared = prepare_texture(texture).expect("the renderer accepts the container");
    assert_eq!(prepared.mips().len(), 1, "only the base level is carried");
    let name = format!("{:?}", prepared.format());
    let level = prepared.mips()[0].bytes.to_vec();
    (name, [prepared.width(), prepared.height()], level)
}

#[test]
fn rgba32_passes_through_with_its_rows_turned_top_first() {
    let bytes = (1..=16).collect();
    let scene = player_scene_texture(&player(RGBA32, [2, 2], false, bytes)).unwrap();
    let (space, rows) = texels(&scene);
    assert_eq!(space, TextureColorSpace::Srgb);
    assert_eq!(
        rows,
        [9, 10, 11, 12, 13, 14, 15, 16, 1, 2, 3, 4, 5, 6, 7, 8]
    );
    let scene = player_scene_texture(&player(RGBA32, [1, 1], true, vec![1, 2, 3, 4])).unwrap();
    assert_eq!(
        texels(&scene),
        (TextureColorSpace::Linear, &[1, 2, 3, 4][..])
    );
}

#[test]
fn rgb24_gains_an_opaque_alpha_and_alpha8_becomes_white_with_that_alpha() {
    let rgb = player(RGB24, [2, 2], false, (1..=12).collect());
    let scene = player_scene_texture(&rgb).unwrap();
    let (_, rows) = texels(&scene);
    assert_eq!(
        rows,
        [7, 8, 9, 255, 10, 11, 12, 255, 1, 2, 3, 255, 4, 5, 6, 255]
    );
    let alpha = player(ALPHA8, [2, 2], true, vec![10, 20, 30, 40]);
    let scene = player_scene_texture(&alpha).unwrap();
    let (space, rows) = texels(&scene);
    assert_eq!(space, TextureColorSpace::Linear);
    let white = |a: u8| [255, 255, 255, a];
    let expected: Vec<u8> = [30, 40, 10, 20].into_iter().flat_map(white).collect();
    assert_eq!(rows, expected);
}

fn block_pair(first: u8, second: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    for fill in [first, second] {
        bytes.extend_from_slice(&[fill, fill, fill, fill, 0x11, 0x22, 0x33, 0x44]);
    }
    bytes
}

#[test]
fn dxt1_becomes_a_bc1_container_whose_block_rows_and_index_rows_are_turned() {
    let scene = player_scene_texture(&player(DXT1, [4, 8], true, block_pair(0xA0, 0xB0))).unwrap();
    let SceneTexture::Encoded {
        bytes, color_space, ..
    } = &scene
    else {
        panic!("expected a container");
    };
    assert_eq!(color_space, &TextureColorSpace::Linear);
    assert_eq!(&bytes[..4], b"DDS ");
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(
        (word(4), word(12), word(16), word(28), word(84)),
        (124, 8, 4, 1, 0x3031_5844)
    );
    assert_eq!((word(128), word(132), word(140)), (71, 3, 1));
    assert_eq!(bytes.len(), 148 + 16, "the payload starts at 148");
    let (name, size, level) = prepared(&scene);
    assert_eq!((name.as_str(), size), ("Bc1RgbaUnorm", [4, 8]));
    let turned = [0xB0, 0xB0, 0xB0, 0xB0, 0x44, 0x33, 0x22, 0x11];
    let other = [0xA0, 0xA0, 0xA0, 0xA0, 0x44, 0x33, 0x22, 0x11];
    assert_eq!(level, [turned, other].concat());
}

#[test]
fn a_texture_that_is_not_linear_takes_the_srgb_form_of_its_block_format() {
    let dxt1 = player(DXT1, [4, 4], false, vec![0; 8]);
    let scene = player_scene_texture(&dxt1).unwrap();
    assert_eq!(prepared(&scene).0, "Bc1RgbaUnormSrgb");
    let dxt5 = player(DXT5, [4, 4], false, vec![0; 16]);
    let scene = player_scene_texture(&dxt5).unwrap();
    assert_eq!(prepared(&scene).0, "Bc3RgbaUnormSrgb");
    let linear = player(DXT5, [4, 4], true, vec![0; 16]);
    assert_eq!(
        prepared(&player_scene_texture(&linear).unwrap()).0,
        "Bc3RgbaUnorm"
    );
}

#[test]
fn dxt5_keeps_its_alpha_block_and_turns_the_alpha_and_colour_index_rows() {
    let alpha_rows: u64 = 1 | 2 << 12 | 3 << 24 | 4 << 36;
    let mut block = vec![0xEE, 0xFF];
    block.extend_from_slice(&alpha_rows.to_le_bytes()[..6]);
    block.extend_from_slice(&[1, 2, 3, 4, 0x11, 0x22, 0x33, 0x44]);
    let scene = player_scene_texture(&player(DXT5, [4, 4], true, block)).unwrap();
    let (name, size, level) = prepared(&scene);
    assert_eq!((name.as_str(), size), ("Bc3RgbaUnorm", [4, 4]));
    let turned: u64 = 4 | 3 << 12 | 2 << 24 | 1 << 36;
    let mut expected = vec![0xEE, 0xFF];
    expected.extend_from_slice(&turned.to_le_bytes()[..6]);
    expected.extend_from_slice(&[1, 2, 3, 4, 0x44, 0x33, 0x22, 0x11]);
    assert_eq!(level, expected);
}

#[test]
fn bc4_is_always_linear_unorm_data() {
    let alpha_rows: u64 = 5 | 6 << 12 | 7 << 24;
    let mut block = vec![0x10, 0x20];
    block.extend_from_slice(&alpha_rows.to_le_bytes()[..6]);
    let scene = player_scene_texture(&player(BC4, [4, 4], false, block)).unwrap();
    let SceneTexture::Encoded { color_space, .. } = &scene else {
        panic!("expected a container");
    };
    assert_eq!(color_space, &TextureColorSpace::Linear);
    let (name, _, level) = prepared(&scene);
    assert_eq!(name, "Bc4RUnorm");
    let turned: u64 = 7 << 12 | 6 << 24 | 5 << 36;
    assert_eq!(level[2..], turned.to_le_bytes()[..6]);
}

#[test]
fn a_width_that_is_not_a_multiple_of_four_rounds_up_to_whole_blocks() {
    let scene = player_scene_texture(&player(DXT1, [5, 4], true, block_pair(1, 2))).unwrap();
    let (_, size, level) = prepared(&scene);
    assert_eq!((size, level.len()), ([5, 4], 16));
    assert_eq!(level[0], 1, "blocks of one row keep their order");
}

#[test]
fn mip_levels_after_the_base_are_left_out() {
    let mut bytes = vec![7; 4 * 4 * 4];
    bytes.extend_from_slice(&[9; 4 * 2 * 2]);
    let mut texture = player(RGBA32, [4, 4], true, bytes);
    texture.mips = 2;
    let scene = player_scene_texture(&texture).unwrap();
    assert_eq!(texels(&scene).1, vec![7; 64]);
}

#[test]
fn formats_the_renderer_cannot_draw_are_reported_by_name() {
    for (format, name) in [
        (25, "BC7"),
        (24, "BC6H"),
        (28, "DXT1Crunched"),
        (62, "TextureFormat 62"),
    ] {
        let error = player_scene_texture(&player(format, [4, 4], false, vec![0; 64])).unwrap_err();
        assert_eq!(
            error,
            PlayerTextureError::Unsupported {
                format: name.into()
            }
        );
        assert_eq!(
            error.to_string(),
            format!("{name} textures are not drawn yet")
        );
    }
}

#[test]
fn short_data_and_unturnable_heights_are_errors() {
    let short = player(RGBA32, [2, 2], false, vec![0; 15]);
    assert!(matches!(
        player_scene_texture(&short),
        Err(PlayerTextureError::Short {
            needed: 16,
            actual: 15,
            ..
        })
    ));
    let short = player(DXT1, [4, 4], false, vec![0; 7]);
    assert!(matches!(
        player_scene_texture(&short),
        Err(PlayerTextureError::Short {
            needed: 8,
            actual: 7,
            ..
        })
    ));
    let uneven = player(DXT5, [4, 6], false, vec![0; 32]);
    assert!(matches!(
        player_scene_texture(&uneven),
        Err(PlayerTextureError::Unaligned { height: 6, .. })
    ));
}

fn bc1_block(c0: u16, c1: u16, indices: u32) -> Vec<u8> {
    let mut block = c0.to_le_bytes().to_vec();
    block.extend_from_slice(&c1.to_le_bytes());
    block.extend_from_slice(&indices.to_le_bytes());
    block
}

fn pixel(rows: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    rows[(y * width + x) * 4..][..4].try_into().unwrap()
}

#[test]
fn decoded_dxt1_has_four_colours_or_three_with_a_transparent_one_by_endpoint_order() {
    const RED: u16 = 0xF800;
    const BLUE: u16 = 0x001F;
    // Row 0 of the stored data (the bottom row) takes indices 0..3 across; the others are all 0.
    let indices = 0b11_10_01_00;
    let four = player(DXT1, [4, 4], false, bc1_block(RED, BLUE, indices));
    let scene = decoded(&four).unwrap();
    let (space, rows) = texels(&scene);
    assert_eq!(space, TextureColorSpace::Srgb);
    // The bottom stored row is the last row once turned top first.
    let row = |x| pixel(rows, 4, x, 3);
    assert_eq!([row(0), row(1)], [[255, 0, 0, 255], [0, 0, 255, 255]]);
    assert_eq!([row(2), row(3)], [[170, 0, 85, 255], [85, 0, 170, 255]]);
    assert_eq!(pixel(rows, 4, 0, 0), [255, 0, 0, 255]);
    let three = player(DXT1, [4, 4], true, bc1_block(BLUE, RED, indices));
    let scene = decoded(&three).unwrap();
    let rows = texels(&scene).1;
    let row = |x| pixel(rows, 4, x, 3);
    assert_eq!([row(2), row(3)], [[127, 0, 127, 255], [0, 0, 0, 0]]);
}

#[test]
fn decoded_dxt5_takes_alpha_from_its_alpha_block_and_always_four_colours() {
    // Alpha endpoints 200 and 100 with index 1 in the first texel and 7 in the second.
    let alpha_bits: u64 = 1 | 7 << 3;
    let mut block = vec![200, 100];
    block.extend_from_slice(&alpha_bits.to_le_bytes()[..6]);
    block.extend(bc1_block(0x001F, 0xF800, 0));
    let scene = decoded(&player(DXT5, [4, 4], true, block)).unwrap();
    let rows = texels(&scene).1;
    assert_eq!(pixel(rows, 4, 0, 3), [0, 0, 255, 100]);
    assert_eq!(pixel(rows, 4, 1, 3), [0, 0, 255, 114]);
}

fn alpha_block(a0: u8, a1: u8, indices: &[u64]) -> Vec<u8> {
    let bits = indices
        .iter()
        .enumerate()
        .fold(0_u64, |bits, (texel, index)| bits | index << (3 * texel));
    let mut block = vec![a0, a1];
    block.extend_from_slice(&bits.to_le_bytes()[..6]);
    block
}

#[test]
fn decoded_bc4_puts_the_value_in_red_with_the_six_step_ramp_below_the_second_endpoint() {
    // a0 <= a1: indices 2..5 interpolate in fifths, 6 is 0 and 7 is 255.
    let block = alpha_block(10, 110, &[0, 1, 2, 5, 6, 7]);
    let scene = decoded(&player(BC4, [4, 4], false, block)).unwrap();
    let rows = texels(&scene).1;
    let reds: Vec<u8> = (0..6)
        .map(|x| pixel(rows, 4, x % 4, 3 - x / 4)[0])
        .collect();
    assert_eq!(reds, [10, 110, 30, 90, 0, 255]);
    assert_eq!(pixel(rows, 4, 0, 3), [10, 0, 0, 255]);
}

#[test]
fn decoded_blocks_beyond_the_texture_edge_are_cropped_and_placed_by_block() {
    // A 5x4 texture: two blocks across, the second holding one visible column.
    let mut bytes = bc1_block(0xF800, 0x001F, 0);
    bytes.extend(bc1_block(0x001F, 0xF800, 0));
    let scene = decoded(&player(DXT1, [5, 4], false, bytes)).unwrap();
    let rows = texels(&scene).1;
    assert_eq!(rows.len(), 5 * 4 * 4);
    assert_eq!(pixel(rows, 5, 3, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(rows, 5, 4, 0), [0, 0, 255, 255]);
}

#[test]
fn decoding_needs_no_aligned_height_but_checks_the_length() {
    let tall = player(DXT5, [4, 6], false, vec![0; 32]);
    assert_eq!(texels(&decoded(&tall).unwrap()).1.len(), 4 * 6 * 4);
    let short = player(DXT5, [4, 4], false, vec![0; 15]);
    assert!(matches!(
        decoded(&short),
        Err(PlayerTextureError::Short {
            needed: 16,
            actual: 15,
            ..
        })
    ));
}

#[test]
fn a_dxt5_colour_block_with_endpoints_in_either_order_has_four_opaque_colours() {
    // c0 <= c1 would select the three-colour mode in DXT1; DXT5 never does.
    let mut block = alpha_block(255, 255, &[0; 16]);
    block.extend(bc1_block(0x001F, 0xF800, 0b11));
    let scene = decoded(&player(DXT5, [4, 4], true, block)).unwrap();
    let rows = texels(&scene).1;
    assert_eq!(pixel(rows, 4, 0, 3), [170, 0, 85, 255]);
}

#[test]
fn decoded_565_endpoints_fill_every_bit_of_each_channel() {
    let green = player(DXT1, [4, 4], true, bc1_block(0x07E0, 0x0000, 0));
    let scene = decoded(&green).unwrap();
    assert_eq!(pixel(texels(&scene).1, 4, 0, 0), [0, 255, 0, 255]);
    let white = player(DXT1, [4, 4], true, bc1_block(0xFFFF, 0x0000, 0));
    let scene = decoded(&white).unwrap();
    assert_eq!(pixel(texels(&scene).1, 4, 0, 0), [255, 255, 255, 255]);
}

const BC7: u32 = 25;

/// A mode 6 block whose sixteen texels are all `rgba`: both endpoints equal, every index zero.
/// Every value is even, so the endpoints' P-bits are clear.
fn solid_bc7_block(rgba: [u8; 4]) -> Vec<u8> {
    assert!(rgba.iter().all(|value| value % 2 == 0));
    let mut bits = 1_u128 << 6;
    let mut at = 7;
    for value in rgba {
        for _ in 0..2 {
            bits |= u128::from(value >> 1) << at;
            at += 7;
        }
    }
    assert_eq!(at, 63, "two clear P-bits and sixteen zero indices follow");
    bits.to_le_bytes().to_vec()
}

#[test]
fn bc7_is_decoded_with_its_block_rows_placed_and_the_texture_turned_top_first() {
    let colours = [
        [10, 20, 30, 254],
        [40, 50, 60, 254],
        [70, 80, 90, 254],
        [100, 110, 120, 254],
    ];
    let bytes: Vec<u8> = colours.iter().flat_map(|c| solid_bc7_block(*c)).collect();
    let scene = decoded(&player(BC7, [8, 8], false, bytes)).unwrap();
    let (space, rows) = texels(&scene);
    assert_eq!(
        space,
        TextureColorSpace::Srgb,
        "a texture that is not linear is sRGB"
    );
    let at = |x: usize, y: usize| rows[(y * 8 + x) * 4..][..4].to_vec();
    assert_eq!(
        at(0, 0),
        colours[2],
        "the first row of the output is the top, the last stored block row"
    );
    assert_eq!(at(7, 0), colours[3]);
    assert_eq!(at(0, 7), colours[0]);
    assert_eq!(at(7, 7), colours[1]);
    assert_eq!(rows.len(), 8 * 8 * 4);
}

#[test]
fn bc7_crops_blocks_past_the_edge_checks_the_length_and_is_never_a_container() {
    let block = solid_bc7_block([200, 100, 50, 254]);
    let bytes: Vec<u8> = (0..4).flat_map(|_| block.clone()).collect();
    let scene = decoded(&player(BC7, [5, 5], true, bytes.clone())).unwrap();
    let (space, rows) = texels(&scene);
    assert_eq!(space, TextureColorSpace::Linear);
    assert_eq!(rows.len(), 5 * 5 * 4);
    assert_eq!(&rows[..4], &[200, 100, 50, 254]);
    assert!(matches!(
        decoded(&player(BC7, [5, 5], true, bytes[..63].to_vec())),
        Err(PlayerTextureError::Short {
            needed: 64,
            actual: 63,
            ..
        })
    ));
    assert_eq!(
        player_scene_texture(&player(BC7, [4, 4], false, block)),
        Err(PlayerTextureError::Unsupported {
            format: "BC7".into()
        })
    );
}

#[test]
fn a_distance_field_is_full_inside_empty_outside_and_half_at_the_edge() {
    let alpha = coverage(4, &[255, 200, 160, 128, 127, 96, 40, 0]);
    assert_eq!(alpha[..3], [255, 255, 255], "inside the glyph");
    assert_eq!(alpha[5..], [0, 0, 0], "outside the glyph");
    assert!(
        (126..=133).contains(&alpha[3]) && (123..=130).contains(&alpha[4]),
        "the edge is about one half: {alpha:?}"
    );
}

#[test]
fn a_distance_field_edge_is_one_texel_wide_whatever_the_spread() {
    // One texel of distance is 255 / (2 * spread) bytes of field.
    for (spread, texel_bytes) in [(1u32, 127.5), (2, 63.75), (4, 31.875), (8, 15.94)] {
        let ramp: Vec<u8> = (0..=255).collect();
        let transition = coverage(spread, &ramp)
            .into_iter()
            .filter(|alpha| (1..255).contains(alpha))
            .count();
        assert!(
            (transition as f32 - texel_bytes).abs() <= 2.,
            "spread {spread}: {transition} bytes of field between empty and full, wanted about {texel_bytes}"
        );
    }
}

#[test]
fn a_distance_field_becomes_coverage_without_going_backwards() {
    let ramp: Vec<u8> = (0..=255).collect();
    let alpha = coverage(3, &ramp);
    assert!(alpha.windows(2).all(|pair| pair[0] <= pair[1]), "{alpha:?}");
    assert_eq!((alpha[0], alpha[255]), (0, 255));
}

#[test]
fn coverage_atlases_pass_through_unchanged() {
    let bytes: Vec<u8> = vec![0, 40, 128, 200, 255];
    assert_eq!(coverage(0, &bytes), bytes);
}

#[test]
fn a_distance_field_keeps_its_rows_top_first() {
    let texture = PlayerTexture {
        distance_spread: 4,
        ..player(ALPHA8, [1, 2], true, vec![255, 0])
    };
    let scene = player_scene_texture(&texture).unwrap();
    let (_, rows) = texels(&scene);
    assert_eq!(rows, [255, 255, 255, 0, 255, 255, 255, 255]);
}
