//! A shadow map written and read through the synchronous flush on a real adapter (lavapipe in CI):
//! a shadow renderer list over a caster draws into a depth-only render texture, and a later
//! forward draw that samples that texture reads the depth the caster wrote and nothing is counted
//! as missing or defaulted.
//!
//! The caster shader has one pass tagged `ShadowCaster` and the receiver one tagged `Alpha` that
//! shows the depth texture's red channel. Every expected texel comes from where the caster is
//! placed and the depth its vertices have, not from the output.

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::unity::synthetic::shaders::{
    albedo_parameters, no_cull, tint_variant, variant,
};
use worldrunner_render_program::unity::synthetic::{
    PassSpec, SyntheticShader, shader as synthetic_shader,
};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, RenderTextureId, RendererListDesc, ShaderProperties, TargetRef,
    TextureRef, WorldRenderTexture, WorldSampler, dimension,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{MeshRenderer, World};

use super::super::report::Report;
use super::flush::host;
use super::gpu::{MESH, Quad, SIDE, shader, target, vector};
use super::support::{camera, renderer_with, snapshot, texture_slot, world};

const ARGB32: u32 = 8;
/// The depth a caster's vertices have in clip space, and so what the shadow map holds under it.
const CASTER_DEPTH: f32 = 0.5;
/// The depth the shadow map is cleared to. Depth is reversed, so the map holds `1 - CLEARED_DEPTH`.
const CLEARED_DEPTH: f32 = 1.0;
/// An 8-bit channel holds a value to within a step.
const STEP: f32 = 1.0 / 255.0;

/// A shader with one pass tagged `ShadowCaster` that places its quad by `_Place` at the depth of
/// the mesh.
pub(super) fn caster_passes() -> SyntheticShader {
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "ShadowCaster")],
            variants: vec![tint_variant(&[], "fs_main")],
        }],
    ))
}

/// The textured camera pass that samples `_Albedo` where the fragment is and shows its red
/// channel as grey.
const RECEIVER: &str = "
struct Material { tint: vec4<f32> }
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
@fragment fn fs_depth(in: VOut) -> @location(0) vec4<f32> {
    let depth = textureSample(albedo, albedo_sampler, in.uv).r;
    return vec4<f32>(depth, depth, depth, 1.0) * material.tint;
}";

/// The same pass reading the texture as a shadow map: the depth `REFERENCE` is compared with the
/// stored depth by the sampler, which gives 1 where the stored depth is at most the reference.
const COMPARING: &str = "
struct Material { tint: vec4<f32> }
struct Draw { place: vec4<f32> }
@group(0) @binding(0) var<uniform> material: Material;
@group(0) @binding(1) var<uniform> draw: Draw;
@group(0) @binding(2) var shadow_map: texture_depth_2d;
@group(0) @binding(3) var shadow_sampler: sampler_comparison;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy * draw.place.xy + draw.place.zw, position.z, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_compare(in: VOut) -> @location(0) vec4<f32> {
    let lit = textureSampleCompare(shadow_map, shadow_sampler, in.uv, 0.25);
    return vec4<f32>(lit, lit, lit, 1.0) * material.tint;
}";

/// A shader with one pass tagged `Alpha` that compares the shadow map `_Albedo` with a depth.
pub(super) fn comparing_passes() -> SyntheticShader {
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![variant(&[], COMPARING, "fs_compare", albedo_parameters())],
        }],
    ))
}

/// A shader with one pass tagged `Alpha` that reads `_Albedo`.
pub(super) fn receiver_passes() -> SyntheticShader {
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![variant(&[], RECEIVER, "fs_depth", albedo_parameters())],
        }],
    ))
}

/// A renderer of the shader `file` whose quad is placed over the part of the target `place` says.
fn quad(
    world: &mut World,
    file: &str,
    place: [f32; 4],
) -> worldrunner_world_simulation::graphics::MaterialId {
    let shader = shader(world, file);
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", [1.0; 4]);
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Vector("_Place".into(), place))
        .unwrap();
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        property_block: Some(block),
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, renderer);
    material
}

/// A `SIDE` by `SIDE` render texture of 32 depth bits and no colour.
fn shadow_map(world: &mut World) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: SIDE,
            height: SIDE,
            format: 0,
            depth_bits: 32,
            mip_count: 1,
            srgb: false,
            dimension: dimension::TEX_2D,
            volume_depth: 1,
            auto_generate_mips: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn bind(world: &mut World, color: Option<RenderTextureId>, depth: Option<RenderTextureId>) {
    world
        .record_offscreen(OffscreenWork::SetRenderTarget {
            color: color.map(TargetRef::Render).into_iter().collect(),
            depth: depth.map(TargetRef::Render),
            mip: 0,
            face: -1,
            slice: -1,
        })
        .unwrap();
}

fn clear(world: &mut World, (color, depth): (bool, bool)) {
    world
        .record_offscreen(OffscreenWork::ClearRenderTarget {
            color,
            depth,
            stencil: false,
            value: [0.0, 0.0, 0.0, 1.0],
            depth_value: CLEARED_DEPTH,
            stencil_value: 0,
        })
        .unwrap();
}

fn shadow_list() -> RendererListDesc {
    RendererListDesc::Shadow {
        cull: 5,
        light_index: 0,
        planes: Vec::new(),
        sphere: [0.0; 4],
        objects_filter: 0,
        projection: 2,
        use_rendering_layer_mask_test: false,
    }
}

fn forward_list() -> RendererListDesc {
    RendererListDesc::Draw {
        cull: 5,
        pass_tags: vec!["Alpha".to_owned()],
        queue_min: 0,
        queue_max: 5000,
        layer_mask: u32::MAX,
        rendering_layer_mask: u32::MAX,
        sorting: 0,
        per_object: 0,
        override_material: None,
        override_pass: 0,
    }
}

/// The commands of a pipeline that draws the casters into `map`, then draws the receivers into
/// `out` with the map as their texture.
fn pipeline(world: &mut World, (map, out): (RenderTextureId, RenderTextureId)) {
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let setup = |world: &mut World, into: RenderTextureId| {
        world
            .record_offscreen(OffscreenWork::SetupCamera {
                camera,
                view: Mat4::IDENTITY.to_cols_array(),
                projection: Mat4::IDENTITY.to_cols_array(),
                target: TargetRef::Render(into),
                buffers: None,
            })
            .unwrap();
    };
    bind(world, None, Some(map));
    clear(world, (false, true));
    setup(world, map);
    world
        .record_offscreen(OffscreenWork::Cull {
            id: 5,
            camera,
            culling_mask: u32::MAX,
            planes: Vec::new(),
            options: 0,
        })
        .unwrap();
    let casters = world.create_renderer_list(shadow_list()).unwrap();
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list: casters })
        .unwrap();
    bind(world, Some(out), None);
    clear(world, (true, false));
    setup(world, out);
    let receivers = world.create_renderer_list(forward_list()).unwrap();
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list: receivers })
        .unwrap();
}

/// Draws a caster over the left half of a shadow map and a receiver of the shader `receiver`
/// over all of an output that reads the map, and returns the flush report and the output's texels.
fn shadowed_scene(receiver: &str) -> (Report, Vec<f32>) {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (out, map) = (target(&mut world, ARGB32), shadow_map(&mut world));
    quad(&mut world, "caster", [0.5, 1.0, -0.5, 0.0]);
    let receiver = quad(&mut world, receiver, [1.0, 1.0, 0.0, 0.0]);
    world
        .edit_material(
            receiver,
            worldrunner_world_simulation::graphics::MaterialEdit::Property(PropertyValue::Texture(
                "_Albedo".into(),
                texture_slot(TextureRef::Render(map)),
            )),
        )
        .unwrap();
    pipeline(&mut world, (map, out));
    let request = world.request_texture_pixels(out, 0, 0, SIDE, SIDE).unwrap();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();
    let read = world.take_texture_pixels(request).unwrap();
    (outcome.report, read)
}

/// Checks the report of a scene whose lists both drew and left no texture missing or defaulted.
fn assert_clean(report: &Report) {
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.lists.shadow_drawn, 1, "{:?}", report.lists);
    assert_eq!(report.lists.drawn, 1, "{:?}", report.lists);
    assert!(
        report.missing_textures.is_empty(),
        "{:?}",
        report.missing_textures
    );
    assert!(
        report.stand_ins.by_property.is_empty(),
        "{:?}",
        report.stand_ins
    );
}

/// Checks that the red and green of every texel of the first two columns is `left` and of the
/// last two `right`.
fn assert_columns(read: &[f32], (left, right): (f32, f32)) {
    for column in 0..SIDE as usize {
        let expected = if column < 2 { left } else { right };
        for row in 0..SIDE as usize {
            let texel = &read[(row * SIDE as usize + column) * 4..][..4];
            assert!(
                (texel[0] - expected).abs() <= STEP && (texel[1] - expected).abs() <= STEP,
                "column {column} row {row}: {texel:?}, expected {expected}"
            );
        }
    }
}

#[test]
fn a_shadow_list_writes_a_depth_texture_that_a_later_forward_draw_reads() {
    let (report, read) = shadowed_scene("receiver");
    assert_clean(&report);
    assert_columns(&read, (CASTER_DEPTH, 1.0 - CLEARED_DEPTH));
}

#[test]
fn a_forward_draw_compares_the_shadow_map_with_a_depth_through_a_comparison_sampler() {
    let (report, read) = shadowed_scene("comparing");
    assert_clean(&report);
    // The reference 0.25 is behind the caster (stored 0.5, and nearer is larger) and in front of the
    // cleared depth 0.
    assert_columns(&read, (0.0, 1.0));
}
