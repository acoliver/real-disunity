//! A texture property the shader declares and no layer sets reads the default texture the shader
//! names, in the dimension the property declares. A one-pass shader writes the texel its `_MainTex`
//! slot reads, so every expected pixel is the default's texel, written out here for each name.

use worldrunner_render_program::unity::synthetic::shaders::{POSITION_AND_UV, no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, binding, shader as synthetic_shader, slot,
};
use worldrunner_unity_serialized::shader::ShaderProperty;
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};

use super::{GREEN, MESH, Mat4, World, draw_mesh, flush, identity_view, left_half, scene, stored};
use super::{Run, shader};

/// The vertex stage and the fragment stages: the texture the slot holds is sampled at the vertex's
/// texture coordinate (2D), at its middle slice (3D), along +Z (cube) or at layer 0 (2D array) and
/// written as it is.
const VERTEX: &str = "
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.uv = uv;
    return out;
}";

const SAMPLE_2D: &str = "
@group(0) @binding(0) var main_tex: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@fragment fn fs_sample(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(main_tex, main_sampler, in.uv);
}";

const SAMPLE_3D: &str = "
@group(0) @binding(0) var main_tex: texture_3d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@fragment fn fs_sample(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(main_tex, main_sampler, vec3<f32>(in.uv, 0.5));
}";

const SAMPLE_CUBE: &str = "
@group(0) @binding(0) var main_tex: texture_cube<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@fragment fn fs_sample(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(main_tex, main_sampler, vec3<f32>(0.0, 0.0, 1.0));
}";

const SAMPLE_ARRAY: &str = "
@group(0) @binding(0) var main_tex: texture_2d_array<f32>;
@group(0) @binding(1) var main_sampler: sampler;
@fragment fn fs_sample(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(main_tex, main_sampler, in.uv, 0);
}";

/// The shader of the file `declared_<default>_<dimension>`: one pass that samples `_MainTex`, a
/// property declared as a texture of dimension code `<dimension>` whose default is
/// `<default>`, with `none` for the empty name.
pub(super) fn passes(file: &str) -> SyntheticShader {
    let (default, dimension) = file
        .strip_prefix("declared_")
        .and_then(|rest| rest.rsplit_once('_'))
        .unwrap_or_else(|| panic!("no declared shader {file}"));
    let dimension: i32 = dimension.parse().unwrap();
    // The dimension codes of a player variant's texture: 4 is 2D, 6 is 3D, 8 a cube, 10 an array.
    let (code, sample) = match dimension {
        // No player variant has a cube-array code (6), so that slot is 2D: only the property's
        // declaration of a cube array matters to the default.
        2 | 6 => (4, SAMPLE_2D),
        3 => (6, SAMPLE_3D),
        4 => (8, SAMPLE_CUBE),
        5 => (10, SAMPLE_ARRAY),
        other => panic!("no sampler for TextureDimension {other}"),
    };
    let mut spec = variant(
        &[],
        &format!("{VERTEX}{sample}"),
        "fs_sample",
        VariantParameters {
            constant_buffers: Vec::new(),
            bindings: vec![
                binding(
                    "_MainTex",
                    slot(0, 0, FRAGMENT),
                    ResourceKind::Texture {
                        sampler: Some(slot(0, 1, FRAGMENT)),
                        dimension: code,
                    },
                ),
                binding("", slot(0, 1, FRAGMENT), ResourceKind::Sampler { state: 0 }),
            ],
        },
    );
    spec.channels = POSITION_AND_UV.to_vec();
    let mut synthetic = no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![spec],
        }],
    ));
    synthetic.shader.parsed.properties = vec![ShaderProperty {
        name: "_MainTex".to_owned(),
        description: String::new(),
        attributes: Vec::new(),
        kind: 4,
        flags: 0,
        default_value: [0.0; 4],
        default_texture: if default == "none" { "" } else { default }.to_owned(),
        texture_dimension: dimension,
    }];
    synthetic
}

/// Draws the left half of the target with a material of `declared_<default>_<dimension>` that
/// sets no texture.
fn draw_unset(default: &str, dimension: i32) -> Run {
    let (mut world, drawn): (World, _) = scene();
    identity_view(&mut world);
    let shader = shader(&mut world, &format!("declared_{default}_{dimension}"));
    let material = world.create_material(shader).unwrap();
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), material, (0, -1));
    flush(&mut world, drawn, &stored([(MESH, left_half())]))
}

/// The 8-bit texel of a colour in the unit range, as a unorm target holds it.
fn texel(colour: [f32; 4]) -> [u8; 4] {
    colour.map(|channel| {
        let scaled = (channel * 255.0).round();
        assert!((0.0..=255.0).contains(&scaled), "{colour:?}");
        // The range was checked, and a rounded value in it is a whole byte.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = scaled as u8;
        byte
    })
}

fn assert_clean(run: &Run) {
    assert!(
        run.outcome.report.failures.is_empty(),
        "{:?}",
        run.outcome.report.failures
    );
    assert_eq!(run.outcome.report.built_by_kind["DrawMesh"], 1);
}

fn assert_reads(default: &str, dimension: i32, colour: [f32; 4]) {
    let run = draw_unset(default, dimension);
    assert_clean(&run);
    let expected = texel(colour);
    assert_eq!(
        run.columns(),
        [expected, expected, GREEN, GREEN],
        "{default} at dimension {dimension}"
    );
    assert_eq!(
        run.outcome.report.stand_ins.by_property,
        std::collections::BTreeMap::from([("_MainTex unset".to_owned(), 1)]),
        "{default} at dimension {dimension}"
    );
}

fn assert_missing(default: &str, dimension: i32) {
    let run = draw_unset(default, dimension);
    let failures: Vec<(&str, usize)> = run
        .outcome
        .report
        .failures
        .iter()
        .map(|(cause, count)| (cause.as_str(), *count))
        .collect();
    assert!(
        failures.len() == 1 && failures[0].0.ends_with(":MissingTexture") && failures[0].1 == 1,
        "{default} at dimension {dimension}: {failures:?}"
    );
    assert_eq!(run.columns(), [GREEN; 4], "the draw is left out");
}

/// The `bump` texel is (0.5, 0.5, 1, 0.5): its RGB is the flat normal under `2 * rgb - 1`. Its
/// alpha is this engine's choice and is not established.
#[test]
fn an_unset_declared_normal_map_reads_the_bump_texel() {
    assert_reads("bump", 2, [0.5, 0.5, 1.0, 0.5]);
}

#[test]
fn the_spellings_of_grey_read_the_grey_texel() {
    for name in ["gray", "grey", "linearGrey"] {
        assert_reads(name, 2, [0.5, 0.5, 0.5, 1.0]);
    }
}

#[test]
fn an_unset_declared_cube_reads_a_cube_of_the_named_default() {
    assert_reads("grey", 4, [0.5, 0.5, 0.5, 1.0]);
    assert_reads("white", 4, [1.0; 4]);
    assert_reads("black", 4, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn an_unset_declared_texture_array_reads_a_layer_of_the_named_default() {
    assert_reads("white", 5, [1.0; 4]);
    assert_reads("bump", 5, [0.5, 0.5, 1.0, 0.5]);
}

#[test]
fn a_default_name_the_table_does_not_list_stays_missing() {
    assert_missing("mystery", 2);
    assert_missing("White", 2);
}

#[test]
fn the_empty_default_name_stays_missing() {
    assert_missing("none", 2);
}

#[test]
fn an_unset_declared_volume_reads_a_one_slice_volume_of_the_named_default() {
    assert_reads("black", 3, [0.0, 0.0, 0.0, 1.0]);
    assert_reads("white", 3, [1.0; 4]);
    assert_reads("grey", 3, [0.5, 0.5, 0.5, 1.0]);
}

/// A property declared as a cube array has no default: the offscreen state uploads no cube-array
/// source, so the property stays missing, as an unknown name does.
#[test]
fn a_declared_cube_array_stays_missing_because_no_cube_array_default_is_uploaded() {
    assert_missing("black", 6);
}
