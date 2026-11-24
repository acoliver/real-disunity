//! A draw that reads a stand-in texture is counted by texture property and by the reason the host
//! gave. The blit shader reads `_Source`; the frame binds the blit's source as `_MainTex`, so
//! `_Source` comes from a layer or from the frame's grey for undeclared slots.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::{Layers, PropertyLayer, TextureInput};
use worldrunner_render_program::unity::offscreen::frame::{
    Material, OffscreenFrame, Record, RenderTextureSpec, UndeclaredTextures,
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
/// A render texture of the frame that no draw of these tests writes unless a test says so.
const RENDERED: TextureId = TextureId(12);

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

/// A blit through the material, into `target`.
fn through_material(target: TextureId) -> Record {
    Record::Blit {
        source: SOURCE,
        target,
        material: Some(MaterialId(1)),
        pass: 0,
    }
}

/// A blit of the source into `target` with no material, which writes `target`.
fn plain_blit(target: TextureId) -> Record {
    Record::Blit {
        source: SOURCE,
        target,
        material: None,
        pass: 0,
    }
}

fn render_textures() -> Vec<RenderTextureSpec> {
    vec![texture(11, [4, 4]), texture(12, [4, 4])]
}

/// `records` over the render textures, with a material whose own layer is `properties` and the
/// grey for undeclared slots.
fn frame_of(properties: PropertyLayer, records: Vec<Record>) -> OffscreenFrame {
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        render_textures: render_textures(),
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
        records,
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
    built_of(properties, vec![through_material(TARGET); blits])
}

fn built_of(properties: PropertyLayer, records: Vec<Record>) -> OffscreenOperations {
    let mut rig = Rig::new();
    setup(&mut rig);
    rig.build(&frame_of(properties, records)).unwrap()
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

#[test]
fn the_highest_layer_that_names_a_slot_decides_why_it_is_a_stand_in() {
    let mut block = PropertyLayer::default();
    block
        .stand_ins
        .insert("_Source".to_owned(), "exists-not-loaded".to_owned());
    let default = with_source(WHITE, Some("unset"));
    assert_eq!(
        Layers::new(&[&block, &default]).stand_in("_Source"),
        Some("exists-not-loaded"),
        "the block names a texture the host does not hold, over the material's default"
    );
    assert_eq!(
        Layers::new(&[&default, &block]).stand_in("_Source"),
        Some("unset"),
        "a layer below the one whose texture is read does not decide"
    );
    let real = with_source(SOURCE, None);
    assert_eq!(
        Layers::new(&[&block, &real]).stand_in("_Source"),
        None,
        "the draw reads the real texture the lower layer holds"
    );
}

#[test]
fn a_draw_that_samples_a_render_texture_nothing_has_written_is_counted() {
    let operations = built_of(with_source(RENDERED, None), vec![through_material(TARGET)]);
    assert_eq!(
        operations.stand_in_draws(),
        [draws("_Source", "unwritten-render-texture", 1)]
    );
    assert_eq!(operations.draws_with_stand_ins(), 1);
}

#[test]
fn a_render_texture_written_earlier_in_the_frame_is_a_real_texture() {
    let operations = built_of(
        with_source(RENDERED, None),
        vec![plain_blit(RENDERED), through_material(TARGET)],
    );
    assert!(operations.stand_in_draws().is_empty());
}

#[test]
fn a_render_texture_an_earlier_frame_wrote_is_a_real_texture() {
    let mut rig = Rig::new();
    setup(&mut rig);
    rig.build(&frame_of(
        with_source(RENDERED, None),
        vec![plain_blit(RENDERED)],
    ))
    .unwrap();
    let operations = rig
        .build(&frame_of(
            with_source(RENDERED, None),
            vec![through_material(TARGET)],
        ))
        .unwrap();
    assert!(operations.stand_in_draws().is_empty());
}

#[test]
fn a_reallocated_texture_is_never_written() {
    let mut rig = Rig::new();
    setup(&mut rig);
    assert!(
        !rig.state.render_texture_is_unwritten(RENDERED),
        "a texture the state does not hold is not one it counts as unwritten"
    );
    let reads = |records| frame_of(with_source(RENDERED, None), records);
    rig.build(&reads(vec![through_material(TARGET)])).unwrap();
    assert!(rig.state.render_texture_is_unwritten(RENDERED));
    assert!(!rig.state.render_texture_is_unwritten(TARGET));
    rig.build(&reads(vec![plain_blit(RENDERED)])).unwrap();
    assert!(!rig.state.render_texture_is_unwritten(RENDERED));
    let mut resized = reads(vec![through_material(TARGET)]);
    resized.render_textures = vec![texture(11, [4, 4]), texture(12, [8, 8])];
    rig.build(&resized).unwrap();
    assert!(
        rig.state.render_texture_is_unwritten(RENDERED),
        "storage made again starts unwritten"
    );
}
