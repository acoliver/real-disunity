use worldrunner_disunity_runtime::PlayerTexture;
use worldrunner_render_plan::{SceneTexture, TextureColorSpace};
use worldrunner_render_scene::texture::prepare_texture;

use crate::textures::{PlayerTextureError, player_scene_texture};

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
        bytes,
    }
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
