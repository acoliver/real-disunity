//! A global texture a script sets directly with `Shader.SetGlobalTexture` keeps its place among
//! the recorded work, on a real adapter (lavapipe in CI).
//!
//! A render texture is filled with a colour, a global is set to it, a list draws a shader that
//! samples the global, and the script then sets the global to no texture before a second list.
//! All of it is recorded before the flush, so the world's global state ends with no texture;
//! the first list must still read the render texture. The shader samples `_Last` at the middle of
//! the texture and writes it as the colour of the quad it draws. Every expected texel comes from
//! the colour the test fills the texture with and from the grey stand-in an unset texture reads.

use worldrunner_render_program::unity::synthetic::shaders::{no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, VERTEX, binding, constant_buffer, shader, slot,
    vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, RenderTextureId, TargetRef, TextureRef,
};

use super::super::flush::OffscreenHost;
use super::flush::host;
use super::gpu::{Quad, SIDE, target};
use super::lights_gpu::{filled, halves, quad, record, two_lists};
use super::support::{assert_close, snapshot, texture_slot, world};

const ARGB32: u32 = 8;
pub(super) const LAST: &str = "_Last";
/// What the test fills the render texture with.
pub(super) const FILL: [f32; 3] = [0.2, 0.4, 0.6];
/// What an unset texture the shader does not declare reads: one texel of 128 in each channel.
pub(super) const GREY: [f32; 3] = [128.0 / 255.0; 3];

/// A vertex stage that places its position by the draw constants `_Place`, and a fragment stage
/// that samples the 2D texture `_Last` at its middle.
const SAMPLING: &str = "
struct Draw { place: vec4<f32> }
@group(0) @binding(1) var<uniform> draw: Draw;
@group(0) @binding(2) var last: texture_2d<f32>;
@group(0) @binding(3) var last_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy * draw.place.xy + draw.place.zw, position.z, 1.0);
    return out;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSampleLevel(last, last_sampler, vec2<f32>(0.5, 0.5), 0.0);
}";

/// The shader of the file `global_sampler`: one pass tagged `Alpha` that reads the global `_Last`.
pub(super) fn sampling_passes() -> SyntheticShader {
    let parameters = VariantParameters {
        constant_buffers: vec![constant_buffer(
            "Draw",
            16,
            vec![vector_member("_Place", 0)],
        )],
        bindings: vec![
            binding("Draw", slot(0, 1, VERTEX), ResourceKind::ConstantBuffer),
            binding(
                LAST,
                slot(0, 2, FRAGMENT),
                ResourceKind::Texture {
                    sampler: Some(slot(0, 3, FRAGMENT)),
                    dimension: 4,
                },
            ),
            binding("", slot(0, 3, FRAGMENT), ResourceKind::Sampler { state: 0 }),
        ],
    };
    no_cull(shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![variant(&[], SAMPLING, "fs_main", parameters)],
        }],
    ))
}

pub(super) fn set_last(world: &mut World, texture: TextureRef) {
    world
        .set_global_property(PropertyValue::Texture(LAST.into(), texture_slot(texture)))
        .unwrap();
}

/// Fills `texture` with `FILL` through recorded work.
pub(super) fn fill(world: &mut World, texture: RenderTextureId) {
    record(
        world,
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(texture)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
    );
    record(
        world,
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: false,
            stencil: false,
            value: [FILL[0], FILL[1], FILL[2], 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        },
    );
}

/// Flushes the world's frame, reads the whole of `drawn` and returns it with the draws the host
/// counted as drawn with a stand-in texture.
fn flush_and_read(mut world: World, drawn: RenderTextureId) -> (Vec<f32>, Option<usize>) {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();
    let undeclared = format!("{LAST} undeclared");
    let stand_ins = outcome
        .report
        .stand_ins
        .by_property
        .get(&undeclared)
        .copied();
    (world.take_texture_pixels(request).unwrap(), stand_ins)
}

#[test]
fn a_list_reads_the_render_texture_a_global_named_when_it_was_set_not_the_value_it_ends_with() {
    let mut world = world();
    let (drawn, source) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    quad(&mut world, "global_sampler", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "global_sampler", 1, [0.5, 1.0, 0.5, 0.0]);
    fill(&mut world, source);
    two_lists(
        &mut world,
        drawn,
        (
            &|world| set_last(world, TextureRef::Render(source)),
            &|world| set_last(world, TextureRef::None),
        ),
    );
    let (read, stand_ins) = flush_and_read(world, drawn);
    // The first list drew with the render texture; only the second, after the script set the
    // global to none, read the stand-in.
    assert_close(&read, &halves(FILL, GREY));
    // The camera render that opens the pipeline draws both renderers before the script sets the
    // global (two stand-ins), and the second list draws one more. Without the ordering the first
    // list would be a fourth.
    assert_eq!(stand_ins, Some(3));
}

/// Take 1 of the two-take tests: fills `source`, sets `_Last` to it and flushes, so the next take
/// starts from a state that holds the texture.
fn first_take(
    world: &mut World,
    host: &mut OffscreenHost,
    drawn: RenderTextureId,
    source: RenderTextureId,
) {
    fill(world, source);
    set_last(world, TextureRef::Render(source));
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let snapshot = snapshot(world);
    host.flush(&snapshot, &Quad).unwrap().apply(world).unwrap();
    world.take_texture_pixels(request).unwrap();
}

/// Flushes the world's frame on `host` and reads the whole of `drawn`.
fn second_take(
    world: &mut World,
    host: &mut OffscreenHost,
    request: u64,
) -> (Vec<f32>, Option<usize>) {
    let snapshot = snapshot(world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(world).unwrap();
    let undeclared = format!("{LAST} undeclared");
    let stand_ins = outcome
        .report
        .stand_ins
        .by_property
        .get(&undeclared)
        .copied();
    (world.take_texture_pixels(request).unwrap(), stand_ins)
}

#[test]
fn draws_before_a_trailing_global_write_read_the_value_the_global_had_when_they_ran() {
    let mut world = world();
    let (drawn, source) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    quad(&mut world, "global_sampler", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "global_sampler", 1, [0.5, 1.0, 0.5, 0.0]);
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    first_take(&mut world, &mut host, drawn, source);
    // Take 2: both quads sample `_Last`, still the filled texture, and the script ends the take
    // by setting it to none. The write follows the take's last record, the pixel read.
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    set_last(&mut world, TextureRef::None);
    let (read, stand_ins) = second_take(&mut world, &mut host, request);
    assert_close(&read, &filled(FILL));
    assert_eq!(stand_ins, None);
}

#[test]
fn a_draw_before_the_first_write_of_a_take_reads_the_value_the_previous_take_left() {
    let mut world = world();
    let (drawn, source) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    quad(&mut world, "global_sampler", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "global_sampler", 1, [0.5, 1.0, 0.5, 0.0]);
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    first_take(&mut world, &mut host, drawn, source);
    // Take 2: the first list draws before the script writes `_Last` in this take; only the
    // second reads none.
    two_lists(
        &mut world,
        drawn,
        (&|_| {}, &|world| set_last(world, TextureRef::None)),
    );
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let (read, _) = second_take(&mut world, &mut host, request);
    assert_close(&read, &halves(FILL, GREY));
}
