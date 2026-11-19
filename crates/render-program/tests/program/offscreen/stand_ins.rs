//! A draw that reads a stand-in texture is counted by texture property and by the reason the host
//! gave. The blit shader reads `_Source`; the frame binds the blit's source as `_MainTex`, so
//! `_Source` comes from a layer or from the frame's grey for undeclared slots.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::unity::offscreen::frame::{
    Material, OffscreenFrame, Record, UndeclaredTextures,
};
use worldrunner_render_program::unity::offscreen::{OffscreenOperations, StandInDraws};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::blit_passes;

use super::rig::{BLIT_QUAD, Rig, blit_quad, layer, source, texture};

const SHADER: ShaderId = ShaderId(2);
const SOURCE: TextureId = TextureId(5);
const GREY: TextureId = TextureId(6);
const WHITE: TextureId = TextureId(7);
const TARGET: TextureId = TextureId(11);

fn setup(rig: &mut Rig) {
    let synthetic = blit_passes("_Source");
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    for id in [5, 6, 7] {
        rig.state
            .set_texture(&mut rig.runtime, &source(id, [1, 1], vec![128; 4]))
            .unwrap();
    }
}

/// A blit through a material whose own layer is `properties`, with the grey for undeclared slots.
fn frame(properties: PropertyLayer, blits: usize) -> OffscreenFrame {
    let record = Record::Blit {
        source: SOURCE,
        target: TARGET,
        material: Some(MaterialId(1)),
        pass: 0,
    };
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        render_textures: vec![texture(11, [4, 4])],
        materials: BTreeMap::from([(
            MaterialId(1),
            Material {
                shader: Some(SHADER),
                keywords: Vec::new(),
                properties,
                fallback: PropertyLayer::default(),
                below_globals: PropertyLayer::default(),
                render_queue: 2000,
            },
        )]),
        records: vec![record; blits],
        undeclared_textures: UndeclaredTextures {
            d2: Some(GREY),
            ..UndeclaredTextures::default()
        },
        ..OffscreenFrame::default()
    }
}

/// A layer that gives `_Source` the texture `texture` and, when `reason` is given, says why it is
/// a stand-in.
fn with_source(texture: TextureId, reason: Option<&str>) -> PropertyLayer {
    let mut properties = layer(&[("_Tint", [1.0; 4])]);
    properties.textures.insert(
        "_Source".to_owned(),
        TextureInput {
            texture,
            sampler: None,
        },
    );
    if let Some(reason) = reason {
        properties
            .stand_ins
            .insert("_Source".to_owned(), reason.to_owned());
    }
    properties
}

fn built(properties: PropertyLayer, blits: usize) -> OffscreenOperations {
    let mut rig = Rig::new();
    setup(&mut rig);
    rig.build(&frame(properties, blits)).unwrap()
}

fn draws(texture: &str, reason: &str, draws: usize) -> StandInDraws {
    StandInDraws {
        texture: texture.to_owned(),
        reason: reason.to_owned(),
        draws,
    }
}

#[test]
fn a_draw_that_reads_a_texture_its_layer_calls_a_stand_in_is_counted_by_property_and_reason() {
    let operations = built(with_source(WHITE, Some("unset")), 2);
    assert_eq!(operations.stand_in_draws(), [draws("_Source", "unset", 2)]);
    assert_eq!(operations.draws_with_stand_ins(), 2);
}

#[test]
fn a_draw_that_reads_a_real_texture_is_not_counted() {
    let operations = built(with_source(WHITE, None), 1);
    assert!(operations.stand_in_draws().is_empty());
    assert_eq!(operations.draws_with_stand_ins(), 0);
}

#[test]
fn a_slot_nothing_sets_reads_the_frames_grey_and_is_undeclared_unless_a_layer_says_why() {
    let operations = built(layer(&[("_Tint", [1.0; 4])]), 1);
    assert_eq!(
        operations.stand_in_draws(),
        [draws("_Source", "undeclared", 1)]
    );
    assert_eq!(operations.draws_with_stand_ins(), 1);

    let mut explained = layer(&[("_Tint", [1.0; 4])]);
    explained
        .stand_ins
        .insert("_Source".to_owned(), "exists-not-loaded".to_owned());
    let operations = built(explained, 1);
    assert_eq!(
        operations.stand_in_draws(),
        [draws("_Source", "exists-not-loaded", 1)]
    );
}
