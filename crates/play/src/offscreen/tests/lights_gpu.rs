//! The main light the host supplies when a frame is drawn on a real adapter (lavapipe in CI): the
//! host's light stands in for a light nothing set, and a light the script sets as a global takes
//! its place at the place in the frame where the script set it.
//!
//! The shaders read `_MainLightColor` or `_MainLightPosition` from a constant buffer named
//! `_PerFrame` and write it as the colour of the quad they draw. Every expected texel comes from
//! the values the test gives the light, the globals and the placement, not from the output.

use std::collections::BTreeSet;

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::unity::synthetic::shaders::{no_cull, variant};
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, VERTEX, binding, constant_buffer, shader, slot,
    vector_member,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, RenderTextureId, RendererListDesc, ShaderProperties, TargetRef,
};
use worldrunner_world_simulation::presentation::{Camera, Light, LightKind, LightSettings};
use worldrunner_world_simulation::{ComponentDesc, MeshRenderer, World};

use super::flush::host;
use super::gpu::shader as world_shader;
use super::gpu::{MESH, Quad, SIDE, target};
use super::support::{assert_close, camera, object, renderer_with, snapshot, world};

const ARGB32: u32 = 8;
pub(super) const COLOR: &str = "_MainLightColor";
const POSITION: &str = "_MainLightPosition";

/// A vertex stage that places its position by the draw constants `_Place`, and fragment stages
/// that write the main light colour, the fourth component of that member as grey, or the main light
/// direction of the per-frame constants as red, green and blue, with an alpha of one.
const PER_FRAME: &str = "
struct PerFrame { color: vec4<f32>, position: vec4<f32> }
struct Draw { place: vec4<f32> }
@group(0) @binding(0) var<uniform> frame: PerFrame;
@group(0) @binding(1) var<uniform> draw: Draw;
struct VOut { @builtin(position) position: vec4<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy * draw.place.xy + draw.place.zw, position.z, 1.0);
    return out;
}
@fragment fn fs_color(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(frame.color.rgb, 1.0);
}
@fragment fn fs_color_w(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(frame.color.www, 1.0);
}
@fragment fn fs_position(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(frame.position.rgb, 1.0);
}";

/// One pass tagged `Alpha` whose fragment stage is `entry` of [`PER_FRAME`].
pub(super) fn per_frame_passes(entry: &'static str) -> SyntheticShader {
    per_frame_passes_named(entry, [COLOR, POSITION])
}

/// As [`per_frame_passes`], with the two members of `_PerFrame` named `members`: first the one
/// `fs_color` writes, then the one `fs_position` writes.
pub(super) fn per_frame_passes_named(entry: &'static str, members: [&str; 2]) -> SyntheticShader {
    let parameters = VariantParameters {
        constant_buffers: vec![
            constant_buffer(
                "_PerFrame",
                32,
                vec![vector_member(members[0], 0), vector_member(members[1], 16)],
            ),
            constant_buffer("Draw", 16, vec![vector_member("_Place", 0)]),
        ],
        bindings: vec![
            binding(
                "_PerFrame",
                slot(0, 0, FRAGMENT),
                ResourceKind::ConstantBuffer,
            ),
            binding("Draw", slot(0, 1, VERTEX), ResourceKind::ConstantBuffer),
        ],
    };
    no_cull(shader(
        &[],
        &[PassSpec {
            tags: vec![("LIGHTMODE", "Alpha")],
            variants: vec![variant(&[], PER_FRAME, entry, parameters)],
        }],
    ))
}

/// The light the host would supply: a directional light shining along `(-0.6, -0.8, 0)`, so the
/// direction toward it is `(0.6, 0.8, 0)`, with `color` and `intensity`.
fn host_light(world: &mut World, color: [f32; 3], intensity: f32) {
    let rotation = Quat::from_rotation_arc(Vec3::Z, Vec3::new(-0.6, -0.8, 0.0));
    let object = object(world, 0, Vec3::ZERO, rotation);
    let settings = LightSettings {
        kind: LightKind::Directional,
        color: [color[0], color[1], color[2], 1.0],
        intensity,
        ..LightSettings::default()
    };
    world
        .add_component(
            object,
            ComponentDesc::Light(Light {
                enabled: true,
                settings,
            }),
        )
        .unwrap();
}

/// A renderer of the shader in `file` on `layer`, over the part of the target `place` says.
pub(super) fn quad(world: &mut World, file: &str, layer: u8, place: [f32; 4]) {
    let shader = world_shader(world, file);
    let material = world.create_material(shader).unwrap();
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Vector("_Place".into(), place))
        .unwrap();
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        property_block: Some(block),
        slot_blocks: std::collections::BTreeMap::new(),
        ..MeshRenderer::default()
    };
    renderer_with(world, (layer, true), MESH, renderer);
}

pub(super) fn record(world: &mut World, work: OffscreenWork) {
    world.record_offscreen(work).unwrap();
}

pub(super) fn set_global(world: &mut World, name: &str, value: [f32; 3]) {
    let value = [value[0], value[1], value[2], 1.0];
    record(
        world,
        OffscreenWork::SetGlobal(PropertyValue::Vector(name.into(), value)),
    );
}

/// What a read of a target should hold when columns 0 and 1 hold `left` and the others `right`,
/// each an 8-bit colour with an alpha of one.
pub(super) fn halves(left: [f32; 3], right: [f32; 3]) -> Vec<f32> {
    let texel = |color: [f32; 3]| [color[0], color[1], color[2], 1.0].map(unorm);
    (0..SIDE)
        .flat_map(|_| {
            (0..SIDE).flat_map(move |column| texel(if column < 2 { left } else { right }))
        })
        .collect()
}

/// What a read of a target should hold when every texel is `color`.
pub(super) fn filled(color: [f32; 3]) -> Vec<f32> {
    halves(color, color)
}

/// A value as an 8-bit texel stores it and a read gives it back.
pub(super) fn unorm(value: f32) -> f32 {
    (value * 255.0).round() / 255.0
}

/// Sets a pipeline up that draws the renderers of layer 0 into the left half of a target and
/// those of layer 1 into the right half, as two lists. `before` records the commands that go
/// before the first draw, and `between` those between the two draws.
pub(super) fn two_lists(
    world: &mut World,
    drawn: RenderTextureId,
    (before, between): (Commands<'_>, Commands<'_>),
) {
    let unrelated = target(world, ARGB32);
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let commands = [
        // A camera render into another target makes the world copy the renderers.
        OffscreenWork::CameraRender {
            camera,
            target: unrelated,
        },
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(drawn)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: true,
            stencil: false,
            value: [0.0, 0.0, 0.0, 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        },
        OffscreenWork::SetupCamera {
            camera,
            view: Mat4::IDENTITY.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target: TargetRef::Render(drawn),
            buffers: None,
        },
        OffscreenWork::Cull {
            id: 5,
            camera,
            culling_mask: 1,
            planes: Vec::new(),
            options: 0,
        },
        OffscreenWork::Cull {
            id: 6,
            camera,
            culling_mask: 2,
            planes: Vec::new(),
            options: 0,
        },
    ];
    for command in commands {
        record(world, command);
    }
    let mut list = |cull| {
        world
            .create_renderer_list(RendererListDesc::Draw {
                cull,
                pass_tags: vec!["Alpha".to_owned()],
                queue_min: 0,
                queue_max: 5000,
                layer_mask: u32::MAX,
                rendering_layer_mask: u32::MAX,
                sorting: 0,
                per_object: 0,
                override_material: None,
                override_pass: 0,
            })
            .unwrap()
    };
    let (first, second) = (list(5), list(6));
    before(world);
    record(world, OffscreenWork::DrawRendererList { list: first });
    between(world);
    record(world, OffscreenWork::DrawRendererList { list: second });
}

/// Flushes the world's frame and reads the whole of `drawn`.
pub(super) fn flush_and_read(mut world: World, drawn: RenderTextureId) -> Vec<f32> {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    world.take_texture_pixels(request).unwrap()
}

/// A world whose light is the host's `intensity` of white, drawing the shader in `file` over the
/// left half through layer 0 and over the right half through layer 1.
pub(super) fn halves_world(
    file: &str,
    (color, intensity): ([f32; 3], f32),
) -> (World, RenderTextureId) {
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, file, 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, file, 1, [0.5, 1.0, 0.5, 0.0]);
    host_light(&mut world, color, intensity);
    (world, drawn)
}

/// Commands a test records at one place of its pipeline.
pub(super) type Commands<'a> = &'a dyn Fn(&mut World);

const A: [f32; 3] = [0.2, 0.4, 0.6];
const B: [f32; 3] = [0.8, 0.6, 0.2];

/// A light of no intensity, the first directional light of a scene whose game lights it by a
/// command buffer.
const DARK: ([f32; 3], f32) = ([1.0, 1.0, 1.0], 0.0);

#[test]
fn a_main_light_color_the_script_sets_replaces_the_hosts_where_the_script_sets_it() {
    let (mut world, drawn) = halves_world("light_color", DARK);
    two_lists(
        &mut world,
        drawn,
        (&|world| set_global(world, COLOR, A), &|world| {
            set_global(world, COLOR, B);
        }),
    );
    // The first list draws with A and the second with B, whatever the host's light is.
    assert_close(&flush_and_read(world, drawn), &halves(A, B));
}

#[test]
fn a_main_light_direction_the_script_sets_replaces_the_hosts_where_the_script_sets_it() {
    let (mut world, drawn) = halves_world("light_position", DARK);
    two_lists(
        &mut world,
        drawn,
        (&|world| set_global(world, POSITION, A), &|world| {
            set_global(world, POSITION, B);
        }),
    );
    assert_close(&flush_and_read(world, drawn), &halves(A, B));
}

#[test]
fn a_main_light_the_script_sets_once_holds_for_the_lists_after_it() {
    let (mut world, drawn) = halves_world("light_color", ([0.0, 1.0, 1.0], 1.0));
    two_lists(
        &mut world,
        drawn,
        (&|world| set_global(world, COLOR, A), &|_| {}),
    );
    assert_close(&flush_and_read(world, drawn), &halves(A, A));
}

#[test]
fn a_script_that_sets_the_color_leaves_the_hosts_direction_in_place() {
    let (mut world, drawn) = halves_world("light_position", DARK);
    two_lists(
        &mut world,
        drawn,
        (&|world| set_global(world, COLOR, A), &|_| {}),
    );
    // The direction toward the light that shines along (-0.6, -0.8, 0).
    assert_close(&flush_and_read(world, drawn), &filled([0.6, 0.8, 0.0]));
}

#[test]
fn a_list_drawn_with_no_light_set_by_the_script_uses_the_hosts_main_light() {
    // The light shines with 0.5 of the colour (0.8, 0.4, 1.2), so its radiance is (0.4, 0.2, 0.6).
    let lit = ([0.8, 0.4, 1.2], 0.5);
    let (mut world, drawn) = halves_world("light_color", lit);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    assert_close(&flush_and_read(world, drawn), &filled([0.4, 0.2, 0.6]));

    let (mut world, drawn) = halves_world("light_position", lit);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    assert_close(&flush_and_read(world, drawn), &filled([0.6, 0.8, 0.0]));
}

/// A world whose one renderer fills the target, drawn by the camera into `first` and then, after
/// the commands `between` records, into `second`.
pub(super) fn camera_renders(
    shader: &str,
    (color, intensity): ([f32; 3], f32),
    (before, between): (Option<[f32; 3]>, Option<[f32; 3]>),
) -> (World, [RenderTextureId; 2]) {
    let mut world = world();
    let targets = [target(&mut world, ARGB32), target(&mut world, ARGB32)];
    quad(&mut world, shader, 0, [1.0, 1.0, 0.0, 0.0]);
    host_light(&mut world, color, intensity);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    if let Some(value) = before {
        set_global(&mut world, COLOR, value);
    }
    record(
        &mut world,
        OffscreenWork::CameraRender {
            camera,
            target: targets[0],
        },
    );
    if let Some(value) = between {
        set_global(&mut world, COLOR, value);
    }
    record(
        &mut world,
        OffscreenWork::CameraRender {
            camera,
            target: targets[1],
        },
    );
    (world, targets)
}

/// Flushes the world's frame and reads both targets.
pub(super) fn flush_and_read_both(
    mut world: World,
    targets: [RenderTextureId; 2],
) -> [Vec<f32>; 2] {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let requests = targets.map(|texture| {
        world
            .request_texture_pixels(texture, 0, 0, SIDE, SIDE)
            .unwrap()
    });
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    requests.map(|request| world.take_texture_pixels(request).unwrap())
}

#[test]
fn a_camera_render_uses_the_main_light_color_the_script_set_before_it() {
    let (world, targets) = camera_renders("light_color", DARK, (Some(A), Some(B)));
    let [first, second] = flush_and_read_both(world, targets);
    assert_close(&first, &filled(A));
    assert_close(&second, &filled(B));
}

#[test]
fn a_camera_render_with_no_light_set_by_the_script_uses_the_hosts_main_light() {
    let lit = ([0.8, 0.4, 1.2], 0.5);
    let (world, targets) = camera_renders("light_color", lit, (None, None));
    let [first, second] = flush_and_read_both(world, targets);
    assert_close(&first, &filled([0.4, 0.2, 0.6]));
    assert_close(&second, &filled([0.4, 0.2, 0.6]));
}

/// The constants and state properties the report names after the frame is flushed.
fn unset_after_flush(mut world: World) -> BTreeSet<String> {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    outcome.report.unset_constants
}

fn names(entries: &[&str]) -> BTreeSet<String> {
    entries.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn the_report_names_the_constants_no_layer_set_for_a_list_draw() {
    // No light in the world and none set by the script: both members of `_PerFrame` stay at zero.
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, "light_color", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "light_color", 1, [0.5, 1.0, 0.5, 0.0]);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    assert_eq!(unset_after_flush(world), names(&[COLOR, POSITION]));
}

#[test]
fn the_report_names_only_the_constants_the_script_left_unset() {
    // The script sets the colour before every draw, camera render included; the world has no
    // light, so the direction is the one no layer sets.
    let mut world = world();
    let drawn = target(&mut world, ARGB32);
    quad(&mut world, "light_color", 0, [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, "light_color", 1, [0.5, 1.0, 0.5, 0.0]);
    set_global(&mut world, COLOR, A);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    assert_eq!(unset_after_flush(world), names(&[POSITION]));
}

#[test]
fn the_report_names_nothing_when_a_layer_set_every_constant() {
    let (mut world, drawn) = halves_world("light_color", DARK);
    two_lists(&mut world, drawn, (&|_| {}, &|_| {}));
    assert_eq!(unset_after_flush(world), BTreeSet::new());
}
