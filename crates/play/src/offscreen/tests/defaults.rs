//! The default textures a shader declares, with predicted names, texels and ids.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::synthetic::shaders::camera_passes;
use worldrunner_unity_serialized::shader::{Shader, ShaderProperty};

use super::super::defaults::{DefaultTexture, declared};
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
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert_eq!(ids[0], DefaultTexture::White.id());
    for id in ids {
        assert_eq!(
            ids::stored_texture(id.0),
            None,
            "{id:?} is not a stored texture's"
        );
    }
}

#[test]
fn only_white_and_black_are_known_by_name() {
    assert_eq!(DefaultTexture::named("white"), Some(DefaultTexture::White));
    assert_eq!(DefaultTexture::named("black"), Some(DefaultTexture::Black));
    for name in ["", "grey", "linearGrey", "bump", "red", "White"] {
        assert_eq!(DefaultTexture::named(name), None, "{name}");
    }
}

#[test]
fn a_shader_declares_a_default_for_each_2d_texture_property_that_names_one_the_host_supplies() {
    let shader = shader_with(vec![
        property("_Main", 4, "white", 2),
        property("_Tiles", 4, "black", 2),
        property("_Normals", 4, "bump", 2),
        property("_Plain", 4, "", 2),
        property("_Layers", 4, "white", 5),
        property("_Tint", 0, "white", 2),
    ]);
    assert_eq!(
        declared(&shader),
        vec![
            ("_Main".to_owned(), DefaultTexture::White),
            ("_Tiles".to_owned(), DefaultTexture::Black),
        ]
    );
}
