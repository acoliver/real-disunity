//! The size of a texture a draw reads, for the constant buffer member `<texture>_TexelSize` that
//! the engine fills with (1 / width, 1 / height, width, height) of the bound texture.

use worldrunner_render_program::ids::{MaterialId, RendererId, TextureId};
use worldrunner_render_program::layers::TextureInput;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::{albedo_parameters, no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, constant_buffer, shader, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::VariantParameters;

use super::camera::{SHADER, TARGET, fixture};
use super::rig::{Rig, layer, pixel, source, texture};

const ALBEDO: TextureId = TextureId(7);
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

fn parameters() -> VariantParameters {
    let mut parameters = albedo_parameters();
    parameters.constant_buffers[0] = constant_buffer(
        "Material",
        32,
        vec![
            vector_member("_Tint", 0),
            vector_member("_Albedo_TexelSize", 16),
        ],
    );
    parameters
}

/// One renderer over a 4x4 target, drawn by the pass that shows the texel size, with an albedo
/// texture of `size` bound to the material.
fn frame(rig: &mut Rig, size: [u32; 2]) -> OffscreenFrame {
    let mut frame = fixture(rig, 1 << 1);
    let synthetic = no_cull(shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![variant(&[], SHOW_TEXEL_SIZE, "fs_main", parameters())],
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
