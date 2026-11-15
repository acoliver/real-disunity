//! A 2D array source: the host uploads it once, a material names it for a texture array property
//! and a draw samples one of its layers through an array view, even when there is one layer.

use std::collections::BTreeMap;

use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::{
    Material, OffscreenFrame, Record, RenderDimension, SourceDimension, SourceTexture,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{
    ARRAY_PATCH, array_patch_parameters, no_cull, variant,
};
use worldrunner_render_program::unity::synthetic::{PassSpec, shader};

use super::rig::{BLIT_QUAD, RGBA8, Rig, blit_quad, pixel, source, texture};

const SHADER: ShaderId = ShaderId(2);
const SOURCE: TextureId = TextureId(5);
const LAYERS: TextureId = TextureId(7);
const TARGET: TextureId = TextureId(11);

/// An array source of `size` texels a layer whose layer `n` is all of `layers[n]`.
fn array(id: TextureId, size: [u32; 2], layers: &[[u8; 4]]) -> SourceTexture {
    let bytes = layers
        .iter()
        .flat_map(|texel| texel.repeat((size[0] * size[1]) as usize))
        .collect();
    SourceTexture {
        id,
        dimension: SourceDimension::D2Array(u32::try_from(layers.len()).unwrap()),
        size,
        graphics_format: RGBA8,
        bytes,
        sampler: None,
    }
}

fn setup(rig: &mut Rig) {
    let synthetic = no_cull(shader(
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
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    rig.state
        .set_texture(&mut rig.runtime, &source(5, [1, 1], vec![255; 4]))
        .unwrap();
}

/// A blit of the shader whose material sets the `_Layers` array property to `LAYERS`.
fn frame() -> OffscreenFrame {
    let mut properties = PropertyLayer::default();
    properties.textures.insert(
        "_Layers".to_owned(),
        TextureInput {
            texture: LAYERS,
            sampler: None,
        },
    );
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

#[test]
fn a_draw_samples_the_layer_it_asks_for_of_an_array_source() {
    let mut rig = Rig::new();
    setup(&mut rig);
    // The shader reads element 2.
    let layers = [
        [10, 0, 0, 255],
        [0, 20, 0, 255],
        [0, 0, 30, 255],
        [40, 40, 40, 255],
    ];
    rig.state
        .set_texture(&mut rig.runtime, &array(LAYERS, [2, 2], &layers))
        .unwrap();
    let result = rig.run(&frame(), &[(TARGET, 0, 0)]).unwrap().remove(0);
    for (x, y) in [(0, 0), (3, 0), (1, 2), (3, 3)] {
        assert_eq!(pixel(&result, 4, (x, y)), [0, 0, 30, 255], "({x}, {y})");
    }
}

/// The shape a default of one layer has: the state holds it, and a draw binds it, as an array.
#[test]
fn a_one_layer_array_is_held_as_an_array_of_one_layer() {
    let mut rig = Rig::new();
    rig.state
        .set_texture(&mut rig.runtime, &array(LAYERS, [1, 1], &[[1, 2, 3, 4]]))
        .unwrap();
    assert_eq!(
        rig.state.texture_dimension(LAYERS),
        Some(RenderDimension::D2Array(1))
    );
    assert_eq!(
        rig.state.texture_view_dimension(LAYERS),
        Some(wgpu::TextureViewDimension::D2Array)
    );
}

#[test]
fn an_array_source_whose_bytes_are_not_its_layers_is_refused() {
    let mut rig = Rig::new();
    let mut short = array(LAYERS, [2, 2], &[[1, 2, 3, 4]; 3]);
    short.bytes.truncate(2 * 2 * 4 * 2);
    let error = rig
        .state
        .set_texture(&mut rig.runtime, &short)
        .expect_err("two layers of bytes are not three");
    assert!(
        matches!(
            error,
            OffscreenError::Program(ProgramError::TextureData { .. })
        ),
        "{error}"
    );
}

#[test]
fn an_array_source_of_no_layers_is_refused() {
    let mut rig = Rig::new();
    let empty = SourceTexture {
        id: LAYERS,
        dimension: SourceDimension::D2Array(0),
        size: [1, 1],
        graphics_format: RGBA8,
        bytes: Vec::new(),
        sampler: None,
    };
    assert!(rig.state.set_texture(&mut rig.runtime, &empty).is_err());
}
