//! A cube slot no layer sets: the URP blit program reflects `_BlitCubeTexture` beside its 2D
//! source, nothing sets it and the shader does not declare it. This engine reads a grey default
//! cube there, which the frame names as `undeclared_textures.cube`.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Material, OffscreenFrame, Record, SourceDimension, SourceTexture, UndeclaredTextures,
};
use worldrunner_render_program::unity::offscreen::{OffscreenError, StandInDraws};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::SyntheticShader;
use worldrunner_render_program::unity::synthetic::shaders::{blit_passes, cube_blit};
use worldrunner_unity_serialized::shader::ShaderProperty;

use super::rig::{BLIT_QUAD, RGBA8, Rig, blit_quad, pixel, source, texture};

const SHADER: ShaderId = ShaderId(2);
const SOURCE: TextureId = TextureId(5);
const GREY_2D: TextureId = TextureId(6);
const GREY_CUBE: TextureId = TextureId(7);
const TARGET: TextureId = TextureId(11);
const SLOT: &str = "_BlitCubeTexture";

/// What the grey default is in RGBA8: 0.5 in each colour channel, opaque.
const GREY_TEXEL: [u8; 4] = [128, 128, 128, 255];

/// A cube source of `edge` texels a side whose six faces are the texels `faces`, in the order +X,
/// -X, +Y, -Y, +Z, -Z.
fn cube(id: TextureId, edge: u32, faces: [[u8; 4]; 6]) -> SourceTexture {
    let bytes = faces
        .iter()
        .flat_map(|texel| texel.repeat((edge * edge) as usize))
        .collect();
    SourceTexture {
        id,
        dimension: SourceDimension::Cube,
        size: [edge, edge],
        graphics_format: RGBA8,
        bytes,
        mips: 1,
        sampler: None,
    }
}

/// A texture property of the player's shaders, a cube: `TextureDimension` 4 in the declaration.
fn declared_cube(name: &str) -> ShaderProperty {
    ShaderProperty {
        name: name.to_owned(),
        description: name.to_owned(),
        attributes: Vec::new(),
        kind: 4,
        flags: 0,
        default_value: [0.0; 4],
        default_texture: String::new(),
        texture_dimension: 4,
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

/// Uploads a cube of grey faces as the cube default.
fn with_grey_cube(rig: &mut Rig) {
    let faces = [GREY_TEXEL; 6];
    rig.state
        .set_texture(&mut rig.runtime, &cube(GREY_CUBE, 1, faces))
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

fn both() -> UndeclaredTextures {
    UndeclaredTextures {
        d2: Some(GREY_2D),
        cube: Some(GREY_CUBE),
        d3: None,
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
fn an_unset_undeclared_cube_slot_reads_the_frames_grey_cube() {
    let mut rig = Rig::new();
    setup(&mut rig, cube_blit(SLOT));
    with_grey_cube(&mut rig);
    let result = rig
        .run(&frame(both()), &[(TARGET, 0, 0)])
        .unwrap()
        .remove(0);
    for (x, y) in [(0, 0), (3, 0), (1, 2), (3, 3)] {
        assert_texel(pixel(&result, 4, (x, y)), GREY_TEXEL);
    }
}

#[test]
fn the_cube_is_read_along_its_own_faces() {
    let mut rig = Rig::new();
    setup(&mut rig, cube_blit(SLOT));
    // Each face has a color of its own; the shader looks along +Z, the fifth face.
    let faces = [
        [10, 0, 0, 255],
        [20, 0, 0, 255],
        [30, 0, 0, 255],
        [40, 0, 0, 255],
        [50, 60, 70, 255],
        [80, 0, 0, 255],
    ];
    rig.state
        .set_texture(&mut rig.runtime, &cube(GREY_CUBE, 2, faces))
        .unwrap();
    let result = rig
        .run(&frame(both()), &[(TARGET, 0, 0)])
        .unwrap()
        .remove(0);
    assert_texel(pixel(&result, 4, (2, 1)), [50, 60, 70, 255]);
}

#[test]
fn the_draw_is_counted_by_property_and_not_as_missing() {
    let mut rig = Rig::new();
    setup(&mut rig, cube_blit(SLOT));
    with_grey_cube(&mut rig);
    let operations = rig.build(&frame(both())).unwrap();
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
fn without_a_cube_default_the_record_is_refused_by_name() {
    let mut rig = Rig::new();
    setup(&mut rig, cube_blit(SLOT));
    // The 2D default does not stand in for a cube slot.
    let only_2d = UndeclaredTextures {
        d2: Some(GREY_2D),
        cube: None,
        d3: None,
    };
    refused(&mut rig, &frame(only_2d), SLOT);
}

#[test]
fn a_2d_slot_is_not_given_the_cube_default() {
    let mut rig = Rig::new();
    setup(&mut rig, blit_passes("_Source"));
    with_grey_cube(&mut rig);
    let only_cube = UndeclaredTextures {
        d2: None,
        cube: Some(GREY_CUBE),
        d3: None,
    };
    refused(&mut rig, &frame(only_cube), "_Source");
}

#[test]
fn a_declared_cube_property_no_layer_sets_is_still_missing() {
    let mut rig = Rig::new();
    let mut synthetic = cube_blit(SLOT);
    synthetic.shader.parsed.properties.push(declared_cube(SLOT));
    setup(&mut rig, synthetic);
    with_grey_cube(&mut rig);
    refused(&mut rig, &frame(both()), SLOT);
}

#[test]
fn a_cube_source_that_is_not_square_is_refused() {
    let mut rig = Rig::new();
    let mut wide = cube(GREY_CUBE, 2, [GREY_TEXEL; 6]);
    wide.size = [2, 1];
    wide.bytes.truncate(6 * 2 * 4);
    let error = rig
        .state
        .set_texture(&mut rig.runtime, &wide)
        .expect_err("a cube has square faces");
    assert!(
        matches!(error, OffscreenError::CubeShape { id, .. } if id == GREY_CUBE),
        "{error}"
    );
}
