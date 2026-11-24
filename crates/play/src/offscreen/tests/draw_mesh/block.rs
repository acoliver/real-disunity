//! A mesh draw's own property block: its values sit between the draw's per-draw values and the
//! material, so they override the material for that draw only. The next draw of the same material
//! without a block reads the material again, and a texture only the block names is sampled.

use std::collections::BTreeMap;

use super::super::super::defaults::StandInLook;
use worldrunner_render_program::unity::synthetic::shaders::{POSITION_AND_UV, no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, binding, shader as synthetic_shader, slot,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_world_simulation::graphics::{
    PropertyValue, RenderTextureId, ShaderProperties, TextureRef, TextureSlot,
};

use super::{
    ARGB32, BLUE, DefaultTexture, GREEN, MESH, Mat4, MaterialId, OffscreenWork, RED, Vec3, World,
    bind_and_clear, flat, flush, flush_without_grey, identity_view, left_half, record, scene,
    shader, stored, target, world,
};

fn tinted(tint: [f32; 4]) -> ShaderProperties {
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Vector("_Tint".into(), tint))
        .unwrap();
    block
}

fn draw_with(world: &mut World, at: Mat4, material: MaterialId, block: Option<ShaderProperties>) {
    record(
        world,
        OffscreenWork::DrawMesh {
            mesh: MESH,
            matrix: at.to_cols_array(),
            material,
            submesh: 0,
            pass: -1,
            block: block.map(Box::new),
            instances: 1,
        },
    );
}

#[test]
fn a_draws_block_overrides_its_material_for_that_draw_only() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_with(
        &mut world,
        Mat4::IDENTITY,
        red,
        Some(tinted([0.0, 0.0, 1.0, 1.0])),
    );
    // One clip unit to the right takes the left half to the right half.
    draw_with(&mut world, Mat4::from_translation(Vec3::X), red, None);

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.columns(), [BLUE, BLUE, RED, RED]);
    assert!(
        run.outcome.report.failures.is_empty(),
        "{:?}",
        run.outcome.report.failures
    );
    assert_eq!(run.outcome.report.built_by_kind["DrawMesh"], 2);
}

#[test]
fn two_draws_of_one_material_read_their_own_blocks() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    draw_with(
        &mut world,
        Mat4::IDENTITY,
        red,
        Some(tinted([0.0, 0.0, 1.0, 1.0])),
    );
    let right = Mat4::from_translation(Vec3::X);
    draw_with(&mut world, right, red, Some(tinted([1.0, 1.0, 1.0, 1.0])));

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.columns(), [BLUE, BLUE, [255; 4], [255; 4]]);
}

/// A pass that places its vertices as given and writes what `_MainTex` holds at the vertex's
/// texture coordinate. No material sets `_MainTex`, so a draw only samples it through its block.
const SAMPLED: &str = "
@group(0) @binding(0) var main_tex: texture_2d<f32>;
@group(0) @binding(1) var main_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_sample(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(main_tex, main_sampler, in.uv);
}";

/// The shader of the file `sampled`: one pass of [`SAMPLED`]. Code 4 is a 2D texture.
pub(super) fn sampled_passes() -> SyntheticShader {
    let mut spec = variant(
        &[],
        SAMPLED,
        "fs_sample",
        VariantParameters {
            constant_buffers: Vec::new(),
            bindings: vec![
                binding(
                    "_MainTex",
                    slot(0, 0, FRAGMENT),
                    ResourceKind::Texture {
                        sampler: Some(slot(0, 1, FRAGMENT)),
                        dimension: 4,
                    },
                ),
                binding("", slot(0, 1, FRAGMENT), ResourceKind::Sampler { state: 0 }),
            ],
        },
    );
    spec.channels = POSITION_AND_UV.to_vec();
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![spec],
        }],
    ))
}

fn sampling(source: RenderTextureId) -> ShaderProperties {
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Texture(
            "_MainTex".into(),
            TextureSlot {
                texture: TextureRef::Render(source),
                scale: [1.0, 1.0],
                offset: [0.0, 0.0],
            },
        ))
        .unwrap();
    block
}

/// The scene of the two block tests: a blue render texture, a target cleared to green, and the
/// `sampled` shader's material.
fn sampling_scene() -> (World, RenderTextureId, RenderTextureId, MaterialId) {
    let mut world = world();
    let source = target(&mut world, ARGB32);
    let drawn = target(&mut world, ARGB32);
    bind_and_clear(&mut world, source);
    record(
        &mut world,
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value: [0.0, 0.0, 1.0, 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        },
    );
    bind_and_clear(&mut world, drawn);
    identity_view(&mut world);
    let shader = shader(&mut world, "sampled");
    let sampled = world.create_material(shader).unwrap();
    (world, source, drawn, sampled)
}

/// On a host without the grey default, the draw without the block has nothing to read for
/// `_MainTex`, so it is left out and counted, and the block's draw is not affected by it.
#[test]
fn a_render_texture_named_only_by_the_block_is_sampled_by_that_draw() {
    let (mut world, source, drawn, sampled) = sampling_scene();
    draw_with(&mut world, Mat4::IDENTITY, sampled, Some(sampling(source)));
    // The same material without the block has no `_MainTex`, so with no grey default to read this
    // draw is left out. The pass ignores the matrix, so the pixels cannot show that; the report's
    // count of the cause does.
    draw_with(&mut world, Mat4::IDENTITY, sampled, None);

    let run = flush_without_grey(&mut world, drawn, &stored([(MESH, left_half())]));

    assert_eq!(run.columns(), [BLUE, BLUE, GREEN, GREEN]);
    assert_eq!(run.outcome.report.built_by_kind["DrawMesh"], 2);
    let failures: Vec<(&str, usize)> = run
        .outcome
        .report
        .failures
        .iter()
        .map(|(cause, count)| (cause.as_str(), *count))
        .collect();
    assert!(
        failures.len() == 1 && failures[0].0.ends_with(":MissingTexture") && failures[0].1 == 1,
        "only the blockless draw is left out, for its missing texture: {failures:?}"
    );
    assert_eq!(run.outcome.report.stand_ins.by_property, BTreeMap::new());
}

/// With the grey default registered, the draw without the block reads it for `_MainTex`, which no
/// layer names and the shader does not declare, instead of being left out. It does not read the
/// block of the draw before it: it paints grey over that draw's blue, and only it is counted as a
/// grey read.
#[test]
fn a_draw_without_the_block_reads_the_grey_default_and_not_the_blocks_render_texture() {
    let (mut world, source, drawn, sampled) = sampling_scene();
    draw_with(&mut world, Mat4::IDENTITY, sampled, Some(sampling(source)));
    draw_with(&mut world, Mat4::IDENTITY, sampled, None);

    let run = flush(&mut world, drawn, &stored([(MESH, left_half())]));

    let [red, green, blue, alpha] = DefaultTexture::Grey.texture(StandInLook::Declared).bytes[..]
    else {
        panic!("the grey default is one RGBA8 texel");
    };
    let grey = [red, green, blue, alpha];
    assert_ne!(grey, BLUE);
    assert_eq!(run.columns(), [grey, grey, GREEN, GREEN]);
    assert_eq!(run.outcome.report.built_by_kind["DrawMesh"], 2);
    assert!(
        run.outcome.report.failures.is_empty(),
        "{:?}",
        run.outcome.report.failures
    );
    assert_eq!(
        run.outcome.report.stand_ins.by_property,
        BTreeMap::from([("_MainTex undeclared".to_owned(), 1)])
    );
}
