//! A procedural draw plans its textures and render state as a camera or list draw does: a texture
//! property no layer sets reads the frame's grey default, and a pass's stencil state tests and
//! writes the stencil buffer of the render texture it draws into.
//!
//! The vertex stage reads no vertex input. It makes two triangles that cover clip x from -1 to a
//! right edge the test chooses and all of clip y, so the target's left half or all of it is drawn.

use std::collections::BTreeMap;
use std::sync::Arc;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::StandInDraws;
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, DrawRaster, Material, OffscreenFrame, ProceduralDraw, Record,
    UndeclaredTextures,
};
use worldrunner_render_program::unity::shader::{PassSelector, ProgramShader};
use worldrunner_render_program::unity::synthetic::shaders::{no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, binding, constant_buffer, fixed, named, shader, slot, vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_unity_serialized::shader_state::ShaderState;

use super::rig::{BLUE, CLEAR, RED, Rig, layer, pixel, source, texture};

const TARGET: TextureId = TextureId(10);
const GREY: TextureId = TextureId(6);
const MARK: ShaderId = ShaderId(1);
const TEST: ShaderId = ShaderId(2);
const SIZE: u32 = 8;

/// What the grey default 2D texture is in RGBA8.
const GREY_TEXEL: [u8; 4] = [128, 128, 128, 255];

/// The shader text: the vertex stage covers clip x from -1 to `right`, the fragment stage writes
/// the tint, times the texture `_Source` at the middle of its image when `textured`.
fn wgsl(right: f32, textured: bool) -> String {
    let (declare, read) = if textured {
        (
            "@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;",
            "textureSample(source, source_sampler, vec2<f32>(0.5, 0.5)) * ",
        )
    } else {
        ("", "")
    };
    format!(
        "
struct Material {{ tint: vec4<f32> }}
@group(0) @binding(0) var<uniform> material: Material;
{declare}
@vertex fn vs_main(@builtin(vertex_index) v: u32) -> @builtin(position) vec4<f32> {{
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>({right:?}, -1.0), vec2<f32>(-1.0, 1.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>({right:?}, -1.0), vec2<f32>({right:?}, 1.0));
    return vec4<f32>(corners[v], 0.0, 1.0);
}}
@fragment fn fs_main() -> @location(0) vec4<f32> {{ return {read}material.tint; }}"
    )
}

fn parameters(textured: bool) -> VariantParameters {
    let mut parameters = VariantParameters {
        constant_buffers: vec![constant_buffer(
            "Material",
            16,
            vec![vector_member("_Tint", 0)],
        )],
        bindings: vec![binding(
            "Material",
            slot(0, 0, FRAGMENT),
            ResourceKind::ConstantBuffer,
        )],
    };
    if textured {
        parameters.bindings.extend([
            binding(
                "_Source",
                slot(0, 1, FRAGMENT),
                ResourceKind::Texture {
                    sampler: Some(slot(0, 2, FRAGMENT)),
                    dimension: 4,
                },
            ),
            binding("", slot(0, 2, FRAGMENT), ResourceKind::Sampler { state: 0 }),
        ]);
    }
    parameters
}

/// A one-pass shader over generated vertices whose pass state is `state`.
fn generated(right: f32, textured: bool, state: impl Fn(&mut ShaderState)) -> ProgramShader {
    generated_from(&wgsl(right, textured), textured, state)
}

/// A one-pass shader of the given WGSL text over generated vertices, whose pass state is `state`.
pub(super) fn generated_from(
    text: &str,
    textured: bool,
    state: impl Fn(&mut ShaderState),
) -> ProgramShader {
    let mut spec = variant(&[], text, "fs_main", parameters(textured));
    spec.channels = Vec::new();
    let mut synthetic = no_cull(shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![spec],
        }],
    ));
    for pass in &mut synthetic.shader.parsed.sub_shaders[0].passes {
        state(&mut pass.state);
    }
    ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap()
}

pub(super) fn whole(texture: TextureId) -> Attachments {
    let attachment = Some(Attachment { texture, mip: 0 });
    Attachments::single(attachment, attachment)
}

pub(super) fn material(shader: ShaderId, tint: [f32; 4], reference: f32) -> Material {
    let mut properties = layer(&[("_Tint", tint)]);
    properties
        .values
        .set("_StencilRef", PropertyValue::Float(reference));
    Material {
        shader: Some(shader),
        keywords: Vec::new(),
        properties,
        fallback: PropertyLayer::default(),
        below_globals: PropertyLayer::default(),
        render_queue: 2000,
    }
}

fn draw(material: u64) -> Record {
    Record::Procedural(Arc::new(ProceduralDraw {
        raster: DrawRaster::default(),
        target: whole(TARGET),
        values: PropertyLayer::default(),
        per_draw: PropertyLayer::default(),
        front_face: wgpu::FrontFace::Cw,
        material: MaterialId(material),
        passes: vec![PassSelector::Index(0)],
        topology: wgpu::PrimitiveTopology::TriangleList,
        vertices: 6,
        instances: 1,
    }))
}

pub(super) fn clear(color: [f64; 4]) -> Record {
    Record::Clear {
        target: whole(TARGET),
        clear: Clear {
            color: Some(color),
            depth: Some(1.0),
            stencil: Some(0),
        },
    }
}

fn grey_rig(undeclared: Option<TextureId>) -> (Rig, OffscreenFrame) {
    let mut rig = Rig::new();
    rig.state.add_shader(TEST, generated(1.0, true, |_| {}));
    rig.state
        .set_texture(&mut rig.runtime, &source(6, [1, 1], GREY_TEXEL.to_vec()))
        .unwrap();
    let frame = OffscreenFrame {
        render_textures: vec![texture(10, [4, 4])],
        materials: BTreeMap::from([(MaterialId(2), material(TEST, [1.0; 4], 0.0))]),
        records: vec![clear(CLEAR.map(|c| f64::from(c) / 255.0)), draw(2)],
        undeclared_textures: UndeclaredTextures {
            d2: undeclared,
            ..UndeclaredTextures::default()
        },
        ..OffscreenFrame::default()
    };
    (rig, frame)
}

#[test]
fn an_unset_undeclared_texture_reads_the_frames_default_in_a_procedural_draw() {
    let (mut rig, frame) = grey_rig(Some(GREY));
    let operations = rig.build(&frame).unwrap();
    assert_eq!(
        operations.stand_in_draws(),
        [StandInDraws {
            texture: "_Source".to_owned(),
            reason: "undeclared".to_owned(),
            draws: 1,
        }]
    );
    assert!(operations.missing_texture_draws().is_empty());
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    for y in 0..4 {
        for x in 0..4 {
            let texel = pixel(&image, 4, (x, y));
            assert!(
                texel[..3].iter().all(|&c| c.abs_diff(GREY_TEXEL[0]) <= 1) && texel[3] == 255,
                "({x}, {y}) is {texel:?}"
            );
        }
    }
}

#[test]
fn without_a_default_a_procedural_draw_reading_an_unset_texture_is_left_out_and_counted() {
    let (mut rig, frame) = grey_rig(None);
    let operations = rig.build(&frame).unwrap();
    assert!(operations.stand_in_draws().is_empty());
    assert!(!operations.missing_texture_draws().is_empty());
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    assert!(image.chunks(4).all(|texel| texel == CLEAR));
}

/// `ZTest Always`, `ZWrite Off`, and a stencil reference read from `_StencilRef`.
fn pass_state(state: &mut ShaderState) {
    state.depth.test = fixed(8.0);
    state.depth.write = fixed(0.0);
    state.stencil.reference = named(0.0, "_StencilRef");
}

/// A scene that marks the stencil of the left half with reference 1 and then draws the whole
/// target in blue where the stencil equals `tested`.
fn stencil_scene(tested: f32) -> Vec<u8> {
    let mut rig = Rig::new();
    rig.state.add_shader(
        MARK,
        generated(0.0, false, |state| {
            pass_state(state);
            state.stencil.both.compare = fixed(8.0);
            state.stencil.both.pass = fixed(2.0);
            state.stencil.write_mask = fixed(255.0);
        }),
    );
    rig.state.add_shader(
        TEST,
        generated(1.0, false, |state| {
            pass_state(state);
            state.stencil.both.compare = fixed(3.0);
            state.stencil.read_mask = fixed(255.0);
        }),
    );
    let mut target = texture(10, [SIZE, SIZE]);
    target.depth_bits = 24;
    let frame = OffscreenFrame {
        render_textures: vec![target],
        materials: BTreeMap::from([
            (MaterialId(1), material(MARK, [1.0, 0.0, 0.0, 1.0], 1.0)),
            (MaterialId(2), material(TEST, [0.0, 0.0, 1.0, 1.0], tested)),
        ]),
        records: vec![clear(CLEAR.map(|c| f64::from(c) / 255.0)), draw(1), draw(2)],
        ..OffscreenFrame::default()
    };
    rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

fn columns_are(image: &[u8], (first, last): (u32, u32), expected: [u8; 4]) -> bool {
    (first..last).all(|x| (0..SIZE).all(|y| pixel(image, SIZE, (x, y)) == expected))
}

#[test]
fn a_procedural_replace_draw_writes_its_reference_and_a_later_equal_draw_covers_those_texels() {
    let image = stencil_scene(1.0);
    assert!(columns_are(&image, (0, 4), BLUE));
    assert!(columns_are(&image, (4, 8), CLEAR));
}

#[test]
fn a_procedural_equal_draw_with_another_reference_draws_nowhere() {
    let image = stencil_scene(2.0);
    assert!(
        columns_are(&image, (0, 4), RED),
        "the marking draw's own color"
    );
    assert!(columns_are(&image, (4, 8), CLEAR));
}
