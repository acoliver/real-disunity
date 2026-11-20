//! The values a host gives every blit: a pass that places the clip-space quad by built-in
//! matrices draws the whole target when the frame supplies them, and nothing when it does not.

use std::collections::BTreeMap;
use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{Material, OffscreenFrame, Record};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::placed::placed_blit;

use super::rig::{
    BLIT_QUAD, BLUE, RED, Rig, ZERO, blit_quad, image, layer, pixel, source, texture,
};

const SHADER: ShaderId = ShaderId(2);
const SOURCE: TextureId = TextureId(5);
const TARGET: TextureId = TextureId(11);
const SIDE: u32 = 4;

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// A matrix that moves every vertex out of the view.
const AWAY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 5.0, 0.0, 1.0,
];

fn matrices(value: [f32; 16]) -> PropertyLayer {
    let mut layer = PropertyLayer::default();
    for name in ["unity_ObjectToWorld", "unity_MatrixVP"] {
        layer.values.set(name, PropertyValue::Matrix(value));
    }
    layer
}

fn rig() -> Rig {
    let mut rig = Rig::new();
    let synthetic = placed_blit("_MainTex");
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    let halves = image([SIDE, SIDE], |x, _| if x < SIDE / 2 { RED } else { BLUE });
    rig.state
        .set_texture(&mut rig.runtime, &source(5, [SIDE, SIDE], halves))
        .unwrap();
    rig
}

fn frame(blit_values: PropertyLayer, globals: PropertyLayer) -> OffscreenFrame {
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        blit_values,
        globals,
        render_textures: vec![texture(11, [SIDE, SIDE])],
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
            pass: 0,
        }],
        ..OffscreenFrame::default()
    }
}

fn drawn(frame: &OffscreenFrame) -> Vec<u8> {
    rig().run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

#[test]
fn a_pass_that_places_the_quad_by_matrices_covers_the_target_when_the_frame_supplies_them() {
    let result = drawn(&frame(matrices(IDENTITY), PropertyLayer::default()));
    for y in 0..SIDE {
        assert_eq!(pixel(&result, SIDE, (0, y)), RED, "left ({y})");
        assert_eq!(pixel(&result, SIDE, (SIDE - 1, y)), BLUE, "right ({y})");
    }
}

#[test]
fn a_pass_that_places_the_quad_by_matrices_draws_nothing_when_no_layer_supplies_them() {
    let result = drawn(&frame(PropertyLayer::default(), PropertyLayer::default()));
    for y in 0..SIDE {
        for x in 0..SIDE {
            assert_eq!(pixel(&result, SIDE, (x, y)), ZERO, "({x}, {y})");
        }
    }
}

#[test]
fn a_global_of_the_same_name_does_not_move_the_quad() {
    let result = drawn(&frame(matrices(IDENTITY), matrices(AWAY)));
    assert_eq!(pixel(&result, SIDE, (0, 0)), RED);
    assert_eq!(pixel(&result, SIDE, (SIDE - 1, SIDE - 1)), BLUE);
}
