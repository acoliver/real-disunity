//! A 3D slot no layer sets: an Old World program reflects `_3DNoiseTexture` beside its 2D source,
//! nothing sets it and the shader does not declare it. The slot reads the frame's grey default,
//! which the frame names as `undeclared_textures.d3`.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Material, OffscreenFrame, Record, SourceDimension, SourceTexture, UndeclaredTextures,
};
use worldrunner_render_program::unity::offscreen::{OffscreenError, StandInDraws};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::SyntheticShader;
use worldrunner_render_program::unity::synthetic::shaders::{blit_passes, volume_blit};
use worldrunner_unity_serialized::shader::ShaderProperty;

use super::rig::{BLIT_QUAD, RGBA8, Rig, blit_quad, pixel, source, texture};

const SHADER: ShaderId = ShaderId(2);
const SOURCE: TextureId = TextureId(5);
const GREY_2D: TextureId = TextureId(6);
const GREY_VOLUME: TextureId = TextureId(8);
const TARGET: TextureId = TextureId(11);
const SLOT: &str = "_3DNoiseTexture";

/// What the grey default is in RGBA8: 0.5 in each colour channel, opaque.
const GREY_TEXEL: [u8; 4] = [128, 128, 128, 255];

/// A 3D source one slice deep whose one texel is `texel`.
fn volume(id: TextureId, texel: [u8; 4]) -> SourceTexture {
    SourceTexture {
        id,
        dimension: SourceDimension::D3(1),
        size: [1, 1],
        graphics_format: RGBA8,
        bytes: texel.to_vec(),
        mips: 1,
        sampler: None,
    }
}

/// A texture property of the player's shaders, a 3D one: `TextureDimension` 3 in the declaration.
fn declared_volume(name: &str) -> ShaderProperty {
    ShaderProperty {
        name: name.to_owned(),
        description: name.to_owned(),
        attributes: Vec::new(),
        kind: 4,
        flags: 0,
        default_value: [0.0; 4],
        default_texture: String::new(),
        texture_dimension: 3,
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

/// Uploads a 3D texture of one texel as the volume default.
fn with_volume(rig: &mut Rig, texel: [u8; 4]) {
    rig.state
        .set_texture(&mut rig.runtime, &volume(GREY_VOLUME, texel))
        .unwrap();
}

fn frame(undeclared_textures: UndeclaredTextures) -> OffscreenFrame {
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        render_textures: vec![texture(11, [4, 4])],
        materials: BTreeMap::from([(
            MaterialId(1),
            Material {
                shader: Some(SHADER),
                keywords: Vec::new(),
                properties: PropertyLayer::default(),
                fallback: PropertyLayer::default(),
                below_globals: PropertyLayer::default(),
                render_queue: 2000,
            },
        )]),
        records: vec![Record::Blit {
            source: SOURCE,
            target: TARGET,
            material: Some(MaterialId(1)),
            pass: 0,
        }],
        undeclared_textures,
        ..OffscreenFrame::default()
    }
}

fn all() -> UndeclaredTextures {
    UndeclaredTextures {
        d2: Some(GREY_2D),
        d3: Some(GREY_VOLUME),
        ..UndeclaredTextures::default()
    }
}

fn refused(rig: &mut Rig, frame: &OffscreenFrame, property: &str) {
    let error = rig.build(frame).err().expect("the record is refused");
    assert!(
        matches!(&error, OffscreenError::MissingTexture { name } if name == property),
        "{error}"
    );
}

fn assert_texel(found: [u8; 4], expected: [u8; 4]) {
    assert!(
        found
            .iter()
            .zip(expected)
            .all(|(found, expected)| found.abs_diff(expected) <= 1),
        "{found:?} against {expected:?}"
    );
}

#[test]
fn an_unset_undeclared_3d_slot_reads_the_frames_grey_volume() {
    let mut rig = Rig::new();
    setup(&mut rig, volume_blit(SLOT));
    with_volume(&mut rig, GREY_TEXEL);
    let result = rig.run(&frame(all()), &[(TARGET, 0, 0)]).unwrap().remove(0);
    for (x, y) in [(0, 0), (3, 0), (1, 2), (3, 3)] {
        assert_texel(pixel(&result, 4, (x, y)), GREY_TEXEL);
    }
}

#[test]
fn the_volume_is_read_from_the_texture_the_frame_names() {
    let mut rig = Rig::new();
    setup(&mut rig, volume_blit(SLOT));
    with_volume(&mut rig, [10, 60, 200, 255]);
    let result = rig.run(&frame(all()), &[(TARGET, 0, 0)]).unwrap().remove(0);
    assert_texel(pixel(&result, 4, (2, 1)), [10, 60, 200, 255]);
}

#[test]
fn the_draw_is_counted_by_property_and_not_as_missing() {
    let mut rig = Rig::new();
    setup(&mut rig, volume_blit(SLOT));
    with_volume(&mut rig, GREY_TEXEL);
    let operations = rig.build(&frame(all())).unwrap();
    assert_eq!(
        operations.stand_in_draws(),
        [StandInDraws {
            texture: SLOT.to_owned(),
            reason: "undeclared".to_owned(),
            draws: 1,
        }]
    );
    assert!(operations.missing_texture_draws().is_empty());
}

#[test]
fn without_a_3d_default_the_record_is_refused_by_name() {
    let mut rig = Rig::new();
    setup(&mut rig, volume_blit(SLOT));
    // The 2D default does not stand in for a 3D slot.
    let only_2d = UndeclaredTextures {
        d2: Some(GREY_2D),
        ..UndeclaredTextures::default()
    };
    refused(&mut rig, &frame(only_2d), SLOT);
}

#[test]
fn a_2d_slot_is_not_given_the_3d_default() {
    let mut rig = Rig::new();
    setup(&mut rig, blit_passes("_Source"));
    with_volume(&mut rig, GREY_TEXEL);
    let only_volume = UndeclaredTextures {
        d3: Some(GREY_VOLUME),
        ..UndeclaredTextures::default()
    };
    refused(&mut rig, &frame(only_volume), "_Source");
}

#[test]
fn a_declared_3d_property_no_layer_sets_is_still_missing() {
    let mut rig = Rig::new();
    let mut synthetic = volume_blit(SLOT);
    synthetic
        .shader
        .parsed
        .properties
        .push(declared_volume(SLOT));
    setup(&mut rig, synthetic);
    with_volume(&mut rig, GREY_TEXEL);
    refused(&mut rig, &frame(all()), SLOT);
}
