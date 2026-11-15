//! The default textures a shader declares, with predicted names, texels and ids.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::frame::SourceDimension;
use worldrunner_render_program::unity::synthetic::shaders::camera_passes;
use worldrunner_unity_serialized::shader::{Shader, ShaderProperty};

use super::super::defaults::{DefaultTexture, declared, declared_names};
use super::super::ids;

fn property(name: &str, kind: i32, default: &str, dimension: i32) -> ShaderProperty {
    ShaderProperty {
        name: name.to_owned(),
        description: String::new(),
        attributes: Vec::new(),
        kind,
        flags: 0,
        default_value: [0.0; 4],
        default_texture: default.to_owned(),
        texture_dimension: dimension,
    }
}

fn shader_with(properties: Vec<ShaderProperty>) -> Shader {
    let mut shader = camera_passes().shader;
    shader.parsed.properties = properties;
    shader
}

#[test]
fn white_is_every_channel_at_one_and_black_has_no_colour_with_full_alpha() {
    let white = DefaultTexture::White.texture();
    assert_eq!(
        (white.size, white.bytes),
        ([1, 1], vec![255, 255, 255, 255])
    );
    let black = DefaultTexture::Black.texture();
    assert_eq!((black.size, black.bytes), ([1, 1], vec![0, 0, 0, 255]));
    assert_eq!(white.graphics_format, black.graphics_format);
}

#[test]
fn the_default_textures_have_names_of_their_own_that_no_stored_texture_takes() {
    let ids: Vec<TextureId> = DefaultTexture::ALL.iter().map(|d| d.texture().id).collect();
    assert_eq!(ids.len(), DefaultTexture::ALL.len());
    let distinct: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "{ids:?}");
    assert_eq!(ids[0], DefaultTexture::White.id());
    for (place, texture) in DefaultTexture::ALL.into_iter().enumerate() {
        assert_eq!(
            texture.id(),
            ids::default_texture(place as u64),
            "{texture:?} is numbered by its place in ALL"
        );
    }
    for id in ids {
        assert_eq!(
            ids::stored_texture(id.0),
            None,
            "{id:?} is not a stored texture's"
        );
    }
}

#[test]
fn the_grey_cube_is_one_grey_texel_on_each_of_six_faces_and_a_cube_named_grey_gets_it() {
    let cube = DefaultTexture::GreyCube.texture();
    assert_eq!((cube.dimension, cube.size), (SourceDimension::Cube, [1, 1]));
    assert_eq!(cube.bytes, [128, 128, 128, 255].repeat(6));
    assert_eq!(
        cube.graphics_format,
        DefaultTexture::Grey.texture().graphics_format
    );
    assert_eq!(
        DefaultTexture::Grey.texture().dimension,
        SourceDimension::D2
    );
    assert_eq!(
        DefaultTexture::named(DefaultTexture::GreyCube.name(), 4),
        Some(DefaultTexture::GreyCube)
    );
}

#[test]
fn the_grey_volume_is_one_grey_texel_one_slice_deep_and_a_volume_named_grey_gets_it() {
    let volume = DefaultTexture::Grey3d.texture();
    assert_eq!(
        (volume.dimension, volume.size),
        (SourceDimension::D3, [1, 1])
    );
    assert_eq!(volume.bytes, DefaultTexture::Grey.texture().bytes);
    assert_eq!(
        volume.graphics_format,
        DefaultTexture::Grey.texture().graphics_format
    );
    assert_eq!(
        DefaultTexture::named(DefaultTexture::Grey3d.name(), 3),
        Some(DefaultTexture::Grey3d)
    );
}

/// The texel of each name, as bytes: Unity's colours (1, 1, 1, 1), (0, 0, 0, 1), (0.5, 0.5, 0.5, 1),
/// the neutral normal (0.5, 0.5, 1, 0.5) and (1, 0, 0, 1), with 0.5 as the byte 128.
const COLOURS: [(&str, [u8; 4]); 8] = [
    ("white", [255, 255, 255, 255]),
    ("black", [0, 0, 0, 255]),
    ("gray", [128, 128, 128, 255]),
    ("grey", [128, 128, 128, 255]),
    ("linearGray", [128, 128, 128, 255]),
    ("linearGrey", [128, 128, 128, 255]),
    ("bump", [128, 128, 255, 128]),
    ("red", [255, 0, 0, 255]),
];

#[test]
fn every_name_unity_knows_gives_its_texel_in_each_dimension_a_draw_can_sample() {
    for (name, texel) in COLOURS {
        for (dimension, shape, faces) in [
            (2, SourceDimension::D2, 1),
            (4, SourceDimension::Cube, 6),
            (5, SourceDimension::D2Array(1), 1),
            (3, SourceDimension::D3, 1),
        ] {
            let texture = DefaultTexture::named(name, dimension)
                .unwrap_or_else(|| panic!("{name} at dimension {dimension} is a default"))
                .texture();
            assert_eq!(
                (texture.dimension, texture.size, texture.bytes),
                (shape, [1, 1], texel.repeat(faces)),
                "{name} at dimension {dimension}"
            );
        }
    }
}

#[test]
fn the_bump_texel_decodes_to_the_neutral_normal_by_both_unpack_paths() {
    let [red, green, blue, alpha] = DefaultTexture::Bump.texture().bytes[..] else {
        panic!("the bump default is one RGBA8 texel");
    };
    let unit = |byte: u8| f32::from(byte) / 255.0;
    // A plain map: 2 rgb - 1.
    let plain = [unit(red), unit(green), unit(blue)].map(|channel| 2.0 * channel - 1.0);
    // A packed map reads x from alpha and y from green, and rebuilds z.
    let [x, y] = [unit(alpha), unit(green)].map(|channel| 2.0 * channel - 1.0);
    let rebuilt = (1.0 - x * x - y * y).max(0.0).sqrt();
    let decoded = [plain[0], plain[1], plain[2], x, y, rebuilt];
    for (found, wanted) in decoded.into_iter().zip([0.0, 0.0, 1.0, 0.0, 0.0, 1.0]) {
        assert!((found - wanted).abs() < 0.01, "{decoded:?}");
    }
}

#[test]
fn the_spellings_of_grey_are_one_texture_and_a_name_is_matched_as_written() {
    assert_eq!(
        DefaultTexture::named("white", 2),
        Some(DefaultTexture::White)
    );
    assert_eq!(
        DefaultTexture::named("black", 2),
        Some(DefaultTexture::Black)
    );
    for name in ["gray", "grey", "linearGray", "linearGrey"] {
        assert_eq!(DefaultTexture::named(name, 2), Some(DefaultTexture::Grey));
        assert_eq!(
            DefaultTexture::named(name, 4),
            Some(DefaultTexture::GreyCube)
        );
        assert_eq!(
            DefaultTexture::named(name, 5),
            Some(DefaultTexture::GreyArray)
        );
        assert_eq!(DefaultTexture::named(name, 3), Some(DefaultTexture::Grey3d));
    }
    for name in ["", "White", "BUMP", "Grey", "lineargrey", "transparent"] {
        for dimension in [2, 3, 4, 5] {
            assert_eq!(DefaultTexture::named(name, dimension), None, "{name}");
        }
    }
}

#[test]
fn a_dimension_a_draw_cannot_sample_gets_no_default() {
    // Unity's TextureDimension: 0 none, 1 any, 6 cube array, and one it does not have.
    for dimension in [0, 1, 6, 7, -1] {
        assert_eq!(
            DefaultTexture::named("white", dimension),
            None,
            "{dimension}"
        );
    }
}

#[test]
fn the_table_has_one_default_for_each_colour_and_shape_and_each_names_itself() {
    let mut seen = Vec::new();
    for texture in DefaultTexture::ALL {
        assert_eq!(
            DefaultTexture::named(texture.name(), texture.dimension()),
            Some(texture)
        );
        let key = (texture.name(), texture.dimension());
        assert!(!seen.contains(&key), "{key:?} twice");
        seen.push(key);
    }
    assert_eq!(seen.len(), 5 * 4);
}

#[test]
fn a_shader_declares_a_default_for_each_texture_property_that_names_one_the_host_supplies_in_its_dimension()
 {
    let shader = shader_with(vec![
        property("_Main", 4, "white", 2),
        property("_Tiles", 4, "black", 2),
        property("_Normals", 4, "bump", 2),
        property("_Plain", 4, "", 2),
        property("_Layers", 4, "white", 5),
        property("_Sky", 4, "grey", 4),
        property("_Volume", 4, "black", 3),
        property("_Stack", 4, "black", 6),
        property("_Mystery", 4, "mystery", 2),
        property("_Tint", 0, "white", 2),
    ]);
    assert_eq!(
        declared(&shader),
        vec![
            ("_Main".to_owned(), DefaultTexture::White),
            ("_Tiles".to_owned(), DefaultTexture::Black),
            ("_Normals".to_owned(), DefaultTexture::Bump),
            ("_Layers".to_owned(), DefaultTexture::WhiteArray),
            ("_Sky".to_owned(), DefaultTexture::GreyCube),
            ("_Volume".to_owned(), DefaultTexture::Black3d),
        ]
    );
}

#[test]
fn a_shader_lists_the_default_name_of_every_2d_texture_property_whether_or_not_the_host_supplies_it()
 {
    let shader = shader_with(vec![
        property("_Main", 4, "white", 2),
        property("_Normals", 4, "bump", 2),
        property("_Plain", 4, "", 2),
        property("_Layers", 4, "white", 5),
        property("_Tint", 0, "white", 2),
    ]);
    assert_eq!(
        declared_names(&shader),
        vec![
            ("_Main".to_owned(), "white".to_owned()),
            ("_Normals".to_owned(), "bump".to_owned()),
            ("_Plain".to_owned(), String::new()),
        ]
    );
}
