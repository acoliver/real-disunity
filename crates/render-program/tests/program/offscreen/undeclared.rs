//! A texture property no layer sets: this engine reads its grey default 2D texture when the shader
//! does not declare the property, and the frame names that texture. The blit shader reads `_Source`
//! while the frame binds the blit's source as `_MainTex`, so `_Source` is set by nothing.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Material, OffscreenFrame, Record, UndeclaredTextures,
};
use worldrunner_render_program::unity::offscreen::{OffscreenError, StandInDraws};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::SyntheticShader;
use worldrunner_render_program::unity::synthetic::shaders::{
    ARRAY_PATCH, array_patch_parameters, blit_passes, no_cull, variant,
};
use worldrunner_render_program::unity::synthetic::{PassSpec, shader};
use worldrunner_unity_serialized::shader::ShaderProperty;

use super::rig::{BLIT_QUAD, Rig, blit_quad, layer, pixel, source, texture};

const SHADER: ShaderId = ShaderId(2);
const SOURCE: TextureId = TextureId(5);
const GREY: TextureId = TextureId(6);
const TARGET: TextureId = TextureId(11);

/// What the grey default 2D texture is in RGBA8.
const GREY_TEXEL: [u8; 4] = [128, 128, 128, 255];

/// A texture property of the player's shaders: 2D, defaulting to `white`.
fn declared(name: &str) -> ShaderProperty {
    ShaderProperty {
        name: name.to_owned(),
        description: name.to_owned(),
        attributes: Vec::new(),
        kind: 4,
        flags: 0,
        default_value: [0.0; 4],
        default_texture: "white".to_owned(),
        texture_dimension: 2,
    }
}

fn setup(rig: &mut Rig, synthetic: SyntheticShader) {
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    for (id, bytes) in [(5, [255, 0, 0, 255]), (6, GREY_TEXEL)] {
        rig.state
            .set_texture(&mut rig.runtime, &source(id, [1, 1], bytes.to_vec()))
            .unwrap();
    }
}

fn frame(pass: usize, grey: Option<TextureId>) -> OffscreenFrame {
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        render_textures: vec![texture(11, [4, 4])],
        materials: BTreeMap::from([(
            MaterialId(1),
            Material {
                shader: Some(SHADER),
                keywords: Vec::new(),
                properties: layer(&[("_Tint", [1.0; 4])]),
                fallback: PropertyLayer::default(),
                below_globals: PropertyLayer::default(),
                render_queue: 2000,
            },
        )]),
        records: vec![Record::Blit {
            source: SOURCE,
            target: TARGET,
            material: Some(MaterialId(1)),
            pass,
        }],
        undeclared_textures: UndeclaredTextures {
            d2: grey,
            ..UndeclaredTextures::default()
        },
        ..OffscreenFrame::default()
    }
}

fn refused(rig: &mut Rig, frame: &OffscreenFrame, property: &str) {
    let error = rig.build(frame).err().expect("the record is refused");
    assert!(
        matches!(&error, OffscreenError::MissingTexture { name } if name == property),
        "{error}"
    );
}

#[test]
fn an_unset_undeclared_texture_reads_the_frames_default() {
    let mut rig = Rig::new();
    setup(&mut rig, blit_passes("_Source"));
    let result = rig
        .run(&frame(0, Some(GREY)), &[(TARGET, 0, 0)])
        .unwrap()
        .remove(0);
    for (x, y) in [(0, 0), (3, 0), (1, 2), (3, 3)] {
        let texel = pixel(&result, 4, (x, y));
        assert!(
            texel[..3].iter().all(|&c| c.abs_diff(GREY_TEXEL[0]) <= 1) && texel[3] == 255,
            "({x}, {y}) is {texel:?}"
        );
    }
}

#[test]
fn the_draw_is_counted_by_property_and_not_as_missing() {
    let mut rig = Rig::new();
    setup(&mut rig, blit_passes("_Source"));
    let operations = rig.build(&frame(0, Some(GREY))).unwrap();
    assert_eq!(
        operations.stand_in_draws(),
        [StandInDraws {
            texture: "_Source".to_owned(),
            reason: "undeclared".to_owned(),
            draws: 1,
        }]
    );
    assert!(operations.missing_texture_draws().is_empty());
}

#[test]
fn without_a_default_the_record_is_refused_as_before() {
    let mut rig = Rig::new();
    setup(&mut rig, blit_passes("_Source"));
    refused(&mut rig, &frame(0, None), "_Source");
}

#[test]
fn a_property_a_layer_sets_is_read_over_the_default() {
    let mut rig = Rig::new();
    setup(&mut rig, blit_passes("_Source"));
    let mut named = frame(0, Some(GREY));
    named.main_texture = "_Source".to_owned();
    let result = rig.run(&named, &[(TARGET, 0, 0)]).unwrap().remove(0);
    assert_eq!(pixel(&result, 4, (1, 1)), [255, 0, 0, 255]);
    assert!(rig.build(&named).unwrap().stand_in_draws().is_empty());
}

#[test]
fn a_declared_property_no_layer_sets_is_still_missing() {
    let mut rig = Rig::new();
    let mut synthetic = blit_passes("_Source");
    synthetic.shader.parsed.properties.push(declared("_Source"));
    setup(&mut rig, synthetic);
    refused(&mut rig, &frame(0, Some(GREY)), "_Source");
}

#[test]
fn a_property_declared_by_another_name_does_not_stop_the_default() {
    let mut rig = Rig::new();
    let mut synthetic = blit_passes("_Source");
    synthetic.shader.parsed.properties.push(declared("_Other"));
    setup(&mut rig, synthetic);
    let operations = rig.build(&frame(0, Some(GREY))).unwrap();
    assert_eq!(operations.stand_in_draws().len(), 1);
}

#[test]
fn a_slot_that_is_not_a_2d_color_texture_is_still_missing() {
    let mut rig = Rig::new();
    // A texture array slot: a 2D colour texture cannot stand in for it.
    let array = no_cull(shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(
                &[],
                ARRAY_PATCH,
                "fs_layer",
                array_patch_parameters(),
            )],
        }],
    ));
    setup(&mut rig, array);
    refused(&mut rig, &frame(0, Some(GREY)), "_Layers");
}
