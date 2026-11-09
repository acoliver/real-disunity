//! `Graphics.Blit`: a material pass over a fullscreen quad, or a plain copy.

use std::collections::BTreeMap;
use worldrunner_render_program::layers::PropertyLayer;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::{Material, OffscreenFrame, Record};
use worldrunner_render_program::unity::shader::ProgramShader;

use super::rig::{
    BLIT_QUAD, BLUE, CYAN, RED, Rig, blit_quad, image, layer, pixel, source, texture,
};
use worldrunner_render_program::unity::synthetic::shaders::blit_passes;

const SHADER: ShaderId = ShaderId(2);
const SOURCE: TextureId = TextureId(5);
const TARGET: TextureId = TextureId(11);

/// A source whose left half is red and whose right half is blue.
fn halves(size: [u32; 2]) -> Vec<u8> {
    image(size, |x, _| if x < size[0] / 2 { RED } else { BLUE })
}

fn setup(rig: &mut Rig, main: &str, size: [u32; 2]) {
    let synthetic = blit_passes(main);
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    rig.state
        .set_texture(&mut rig.runtime, &source(5, size, halves(size)))
        .unwrap();
}

fn frame(tint: [f32; 4], pass: usize, size: [u32; 2]) -> OffscreenFrame {
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        render_textures: vec![texture(11, size)],
        materials: BTreeMap::from([(
            MaterialId(1),
            Material {
                shader: Some(SHADER),
                keywords: Vec::new(),
                properties: layer(&[("_Tint", tint)]),
                fallback: PropertyLayer::default(),
                render_queue: 2000,
            },
        )]),
        records: vec![Record::Blit {
            source: SOURCE,
            target: TARGET,
            material: Some(MaterialId(1)),
            pass,
        }],
        ..OffscreenFrame::default()
    }
}

fn blit(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<u8> {
    rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

#[test]
fn a_blit_with_a_material_pass_draws_the_source_through_the_pass() {
    let mut rig = Rig::new();
    setup(&mut rig, "_MainTex", [4, 4]);
    // Pass 0 multiplies the source by the tint: the tint drops red, so red becomes black and blue
    // stays blue. The source and target have the same size, so each texel is sampled at its center.
    let result = blit(&mut rig, &frame([0.0, 1.0, 1.0, 1.0], 0, [4, 4]));
    for y in 0..4 {
        assert_eq!(pixel(&result, 4, (0, y)), [0, 0, 0, 255], "left ({y})");
        assert_eq!(pixel(&result, 4, (1, y)), [0, 0, 0, 255]);
        assert_eq!(pixel(&result, 4, (2, y)), BLUE);
        assert_eq!(pixel(&result, 4, (3, y)), BLUE);
    }
}

#[test]
fn the_pass_is_chosen_by_index() {
    let mut rig = Rig::new();
    setup(&mut rig, "_MainTex", [4, 4]);
    // Pass 2 inverts: red becomes cyan and blue becomes yellow.
    let result = blit(&mut rig, &frame([1.0; 4], 2, [4, 4]));
    assert_eq!(pixel(&result, 4, (0, 0)), CYAN);
    assert_eq!(pixel(&result, 4, (3, 3)), [255, 255, 0, 255]);
}

#[test]
fn the_pass_the_shader_lacks_is_a_named_error() {
    let mut rig = Rig::new();
    setup(&mut rig, "_MainTex", [4, 4]);
    let error = rig.build(&frame([1.0; 4], 3, [4, 4])).err().unwrap();
    assert!(
        matches!(
            &error,
            OffscreenError::Resolve(
                worldrunner_render_program::unity::shader::ResolveError::NoPass { .. }
            )
        ),
        "{error}"
    );
}

#[test]
fn the_source_texel_size_is_bound_when_the_pass_reads_it() {
    let mut rig = Rig::new();
    setup(&mut rig, "_MainTex", [4, 2]);
    // Pass 1 shows whether the texel size is (1/4, 1/2, 4, 2): red, green and blue are set when
    // the width, height and one over the width agree.
    let result = blit(&mut rig, &frame([1.0; 4], 1, [4, 2]));
    assert_eq!(pixel(&result, 4, (2, 1)), [255, 255, 255, 255]);
}

#[test]
fn the_main_texture_name_is_configurable_and_defaults_to_the_public_name() {
    let mut rig = Rig::new();
    // The shader reads `_Source` and `_Source_TexelSize`.
    setup(&mut rig, "_Source", [4, 2]);
    let mut named = frame([1.0; 4], 1, [4, 2]);
    named.main_texture = "_Source".to_owned();
    let result = blit(&mut rig, &named);
    assert_eq!(pixel(&result, 4, (0, 0)), [255, 255, 255, 255]);
    // With the default name the source is bound as `_MainTex`, which this shader does not read.
    let error = rig.build(&frame([1.0; 4], 1, [4, 2])).err().unwrap();
    assert!(
        matches!(&error, OffscreenError::MissingTexture { name } if name == "_Source"),
        "{error}"
    );
}

#[test]
fn a_blit_without_a_material_copies_a_source_of_the_same_size_and_format() {
    let mut rig = Rig::new();
    setup(&mut rig, "_MainTex", [4, 4]);
    let mut copy = frame([1.0; 4], 0, [4, 4]);
    copy.records = vec![Record::Blit {
        source: SOURCE,
        target: TARGET,
        material: None,
        pass: 0,
    }];
    let result = blit(&mut rig, &copy);
    assert_eq!(result, halves([4, 4]));
}

#[test]
fn a_blit_with_a_material_needs_the_frame_to_name_a_blit_mesh() {
    let mut rig = Rig::new();
    setup(&mut rig, "_MainTex", [4, 4]);
    let mut without = frame([1.0; 4], 0, [4, 4]);
    without.blit_mesh = None;
    let error = rig.build(&without).err().unwrap();
    assert!(matches!(error, OffscreenError::NoBlitMesh), "{error}");
}

#[test]
fn a_blit_into_a_texture_the_frame_does_not_declare_is_a_named_error() {
    let mut rig = Rig::new();
    setup(&mut rig, "_MainTex", [4, 4]);
    let mut stray = frame([1.0; 4], 0, [4, 4]);
    stray.records = vec![Record::Blit {
        source: TARGET,
        target: SOURCE,
        material: Some(MaterialId(1)),
        pass: 0,
    }];
    let error = rig.build(&stray).err().unwrap();
    assert!(
        matches!(error, OffscreenError::NotRenderTexture(id) if id == SOURCE),
        "{error}"
    );
}
