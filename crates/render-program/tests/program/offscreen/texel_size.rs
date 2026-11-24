//! The size of a texture a draw reads, for the constant buffer member `<texture>_TexelSize` that
//! the engine fills with (1 / width, 1 / height, width, height) of the bound texture.

use std::collections::BTreeSet;
use std::sync::Arc;
use worldrunner_render_program::ids::{MaterialId, RendererId, TextureId};
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::properties::PropertyValue;

use worldrunner_render_program::unity::offscreen::frame::{GlobalState, OffscreenFrame, Record};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{
    albedo_parameters, no_cull, tint_parameters, variant,
};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, constant_buffer, shader, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::VariantParameters;

use super::camera::{SHADER, TARGET, fixture};
use super::rig::{Rig, layer, pixel, source, texture};

const ALBEDO: TextureId = TextureId(7);
/// The 1x1 texture a draw reads for a texture that is none: the frame's `undeclared_textures.d2`.
const STAND_IN: TextureId = TextureId(8);
const WHITE: [u8; 4] = [255; 4];

/// A pass that shows `_Albedo_TexelSize` of a 4x2 texture as (1, 1, 1, 1): each member is scaled
/// by the size it should hold. It reads the texture too, so the pass binds it.
const SHOW_TEXEL_SIZE: &str = "
struct Material { tint: vec4<f32>, albedo_texel_size: vec4<f32> }
struct Draw { place: vec4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
@group(0) @binding(2) var albedo: texture_2d<f32>;
@group(0) @binding(3) var albedo_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy * draw.place.xy + draw.place.zw, position.z, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let t = material.albedo_texel_size;
    return vec4<f32>(t.x * 4.0, t.y * 2.0, t.z * 0.25, t.w * 0.5)
        + textureSample(albedo, albedo_sampler, in.uv) * 0.0;
}";

/// The `_G_TexelSize` member itself as the colour, for a texture `_G` the pass declares the member
/// for and never samples, the way a shadow caster reads the size of a global texture.
const SHOW_UNSAMPLED_TEXEL_SIZE: &str = "
struct Material { tint: vec4<f32>, g_texel_size: vec4<f32> }
struct Draw { place: vec4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
@vertex fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position.xy * draw.place.xy + draw.place.zw, position.z, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4<f32> {
    return material.g_texel_size;
}";

fn parameters(base: VariantParameters, member: &str) -> VariantParameters {
    let mut parameters = base;
    parameters.constant_buffers[0] = constant_buffer(
        "Material",
        32,
        vec![vector_member("_Tint", 0), vector_member(member, 16)],
    );
    parameters
}

/// One renderer over a 4x4 target, drawn by `wgsl` with `parameters`, and a texture of `size`
/// uploaded as `ALBEDO` that no layer binds yet.
fn base(
    rig: &mut Rig,
    wgsl: &str,
    parameters: VariantParameters,
    size: [u32; 2],
) -> OffscreenFrame {
    let mut frame = fixture(rig, 1 << 1);
    let synthetic = no_cull(shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![variant(&[], wgsl, "fs_main", parameters)],
        }],
    ));
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    frame.render_textures = vec![texture(10, [4, 4])];
    frame.renderers.get_mut(&RendererId(1)).unwrap().per_draw =
        layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    let bytes = vec![0; (size[0] * size[1] * 4) as usize];
    rig.state
        .set_texture(&mut rig.runtime, &source(7, size, bytes))
        .unwrap();
    frame
}

/// `base` for the pass that samples `_Albedo` and shows its texel size, with the texture bound
/// to the material.
fn frame(rig: &mut Rig, size: [u32; 2]) -> OffscreenFrame {
    let parameters = parameters(albedo_parameters(), "_Albedo_TexelSize");
    let mut frame = base(rig, SHOW_TEXEL_SIZE, parameters, size);
    let properties = &mut frame.materials.get_mut(&MaterialId(1)).unwrap().properties;
    properties.values = layer(&[("_Tint", [1.0; 4])]).values;
    properties.textures.insert(
        "_Albedo".to_owned(),
        TextureInput {
            texture: ALBEDO,
            sampler: None,
        },
    );
    frame
}

fn center(rig: &mut Rig, frame: &OffscreenFrame) -> [u8; 4] {
    let image = rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    pixel(&image, 4, (2, 2))
}

#[test]
fn a_draws_texel_size_member_holds_the_size_of_the_texture_bound_under_its_name() {
    let mut rig = Rig::new();
    let frame = frame(&mut rig, [4, 2]);
    assert_eq!(center(&mut rig, &frame), WHITE);
}

#[test]
fn the_texel_size_follows_the_size_of_the_texture() {
    let mut rig = Rig::new();
    // 8x8: (1/8, 1/8, 8, 8) shown as (0.5, 0.25, 2, 4), of which the last two saturate.
    let frame = frame(&mut rig, [8, 8]);
    assert_eq!(center(&mut rig, &frame), [128, 64, 255, 255]);
}

#[test]
fn a_texel_size_a_layer_sets_is_the_one_the_draw_reads() {
    let mut rig = Rig::new();
    let mut frame = frame(&mut rig, [4, 2]);
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .properties
        .values
        .set(
            "_Albedo_TexelSize",
            PropertyValue::Vector([0.125, 0.25, 0.0, 0.0]),
        );
    assert_eq!(center(&mut rig, &frame), [128, 128, 0, 0]);
}

/// `base` for the pass that reads `_G_TexelSize` and samples no `_G`; the caller decides which
/// layer, if any, names a texture `_G`. The frame has a 1x1 stand-in for a texture that is none.
fn unsampled(rig: &mut Rig) -> OffscreenFrame {
    let parameters = parameters(tint_parameters(), "_G_TexelSize");
    let mut frame = base(rig, SHOW_UNSAMPLED_TEXEL_SIZE, parameters, [4, 2]);
    rig.state
        .set_texture(&mut rig.runtime, &source(8, [1, 1], vec![128; 4]))
        .unwrap();
    frame.undeclared_textures.d2 = Some(STAND_IN);
    let properties = &mut frame.materials.get_mut(&MaterialId(1)).unwrap().properties;
    properties.values = layer(&[("_Tint", [1.0; 4])]).values;
    frame
}

fn global_g(frame: &mut OffscreenFrame) {
    frame.globals.textures.insert(
        "_G".to_owned(),
        TextureInput {
            texture: ALBEDO,
            sampler: None,
        },
    );
}

#[test]
fn a_texel_size_comes_from_the_texture_a_layer_names_though_the_draw_never_samples_it() {
    let mut rig = Rig::new();
    let mut frame = unsampled(&mut rig);
    global_g(&mut frame);
    // (1/4, 1/2, 4, 2), the last two saturated.
    assert_eq!(center(&mut rig, &frame), [64, 128, 255, 255]);
}

#[test]
fn a_texel_size_no_layer_names_a_texture_for_is_zero() {
    let mut rig = Rig::new();
    let frame = unsampled(&mut rig);
    assert_eq!(center(&mut rig, &frame), [0; 4]);
}

#[test]
fn an_explicit_texel_size_wins_over_the_texture_the_name_resolves_to() {
    let mut rig = Rig::new();
    let mut frame = unsampled(&mut rig);
    global_g(&mut frame);
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .properties
        .values
        .set(
            "_G_TexelSize",
            PropertyValue::Vector([0.125, 0.25, 0.0, 0.0]),
        );
    assert_eq!(center(&mut rig, &frame), [32, 64, 0, 0]);
}

fn none_g() -> PropertyLayer {
    PropertyLayer {
        none_textures: BTreeSet::from(["_G".to_owned()]),
        ..PropertyLayer::default()
    }
}

#[test]
fn a_texture_set_to_none_has_the_texel_size_of_the_stand_in_the_draw_reads() {
    let mut rig = Rig::new();
    let mut frame = unsampled(&mut rig);
    frame.globals = none_g();
    assert_eq!(center(&mut rig, &frame), WHITE);
}

#[test]
fn a_layer_that_sets_a_texture_to_none_leaves_the_texture_a_lower_layer_names_the_one_read() {
    let mut rig = Rig::new();
    let mut frame = unsampled(&mut rig);
    global_g(&mut frame);
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .properties
        .none_textures
        .insert("_G".to_owned());
    // The draw binds the global 4x2 texture, so the size is its (1/4, 1/2, 4, 2).
    assert_eq!(center(&mut rig, &frame), [64, 128, 255, 255]);
}

#[test]
fn a_globals_record_that_sets_a_texture_to_none_replaces_the_texture_the_globals_had() {
    let mut rig = Rig::new();
    let mut frame = unsampled(&mut rig);
    global_g(&mut frame);
    frame.records.insert(
        0,
        Record::Globals(Arc::new(GlobalState {
            values: none_g(),
            keywords: Vec::new(),
        })),
    );
    assert_eq!(center(&mut rig, &frame), WHITE);
}
