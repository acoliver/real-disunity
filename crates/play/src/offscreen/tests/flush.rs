//! The synchronous GPU flush on a real adapter (lavapipe in CI): a camera render into a height
//! map, a dispatch that copies it into a compute buffer, and the bytes the world gets back.
//!
//! The kernel is the shape of the terrain read-back: it reads an R16 float render texture and
//! writes a storage buffer, and a vector in a constant buffer scales the values. It is stored as
//! a serialized compute shader object in a synthetic data directory and reaches the host through
//! the player's decoder, so the flush runs a kernel built the way a player's are. Every expected
//! value comes from the tint, the clear color and the scale, not from the output.

use std::collections::BTreeMap;
use std::path::Path;

use glam::{Quat, Vec3};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, ComputeShaderId, OffscreenWork, PropertyValue, RenderTextureId,
    ShaderProperties,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{MeshRenderer, World};

use super::super::adapter::OffscreenAdapter;
use super::super::compute::ComputeAdapter;
use super::super::flush::{FlushOutcome, OffscreenHost};
use super::super::frame::Settings;
use super::super::kernels::PlayerKernels;
use super::gpu::{BareQuad, MESH, Quad, SIDE, Synthetic, gpu, shader, target, vector};
use super::kernels::{copy_object, text_object, write_compute};
use super::support::{
    camera, compute_buffer, compute_shader, dispatch, render_texture, renderer_with, snapshot,
    world,
};

/// `GraphicsFormat` 45, one 16-bit float channel.
const HEIGHT: u32 = 45;
/// The red the camera's material draws over the left half, and the red it clears to.
const DRAWN: f32 = 0.5;
const CLEAR: f32 = 0.25;
/// What the kernel multiplies every height by.
const SCALE: f32 = 2.0;

/// A host whose kernels are read from the compute shader objects under `directory`.
fn host(directory: &Path) -> OffscreenHost {
    let settings = Settings {
        light_modes: vec!["Alpha".to_owned()],
        ..Settings::default()
    };
    host_with(directory, settings)
}

fn host_with(directory: &Path, settings: Settings) -> OffscreenHost {
    let gpu = gpu();
    OffscreenHost::new(
        gpu.runtime,
        gpu.arena,
        OffscreenAdapter::with_supply(Synthetic, settings),
        ComputeAdapter::new(PlayerKernels::new(directory)),
    )
}

/// A camera that clears to `CLEAR` in red and draws `DRAWN` over the left half of its target.
fn camera_render(world: &mut World, into: RenderTextureId) {
    camera_render_of(world, into, "camera");
}

/// As `camera_render`, with the shader the file `shader_file` names.
fn camera_render_of(world: &mut World, into: RenderTextureId, shader_file: &str) {
    let camera = camera(
        world,
        Camera {
            background: [f64::from(CLEAR), 0.0, 1.0, 1.0],
            ..Camera::default()
        },
        Vec3::ZERO,
        Quat::IDENTITY,
    );
    let shader = shader(world, shader_file);
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", [DRAWN, 0.0, 0.0, 1.0]);
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Vector(
            "_Place".into(),
            [0.5, 1.0, -0.5, 0.0],
        ))
        .unwrap();
    let placed = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        property_block: Some(block),
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, placed);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: into,
        })
        .unwrap();
}

/// A compute shader the kernels are found for, scaling by `SCALE`.
fn scaling(world: &mut World) -> ComputeShaderId {
    let shader = compute_shader(world, "heights");
    world
        .set_compute_property(
            shader,
            None,
            PropertyValue::Vector("_Scale".into(), [SCALE, 0.0, 0.0, 0.0]),
        )
        .unwrap();
    shader
}

fn floats(world: &World, buffer: ComputeBufferId) -> Vec<f32> {
    world
        .compute_buffer(buffer)
        .unwrap()
        .contents
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect()
}

/// The values the kernel writes for a height map the camera drew: the left two columns of each
/// row are drawn, the rest cleared.
fn expected() -> Vec<f32> {
    (0..SIDE * SIDE)
        .map(|index| {
            if index % SIDE < SIDE / 2 {
                DRAWN * SCALE
            } else {
                CLEAR * SCALE
            }
        })
        .collect()
}

#[test]
fn a_camera_render_into_a_height_map_then_a_dispatch_that_copies_it_writes_the_buffer_back() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    assert_eq!(outcome.writes.len(), 1);
    outcome.apply(&mut world).unwrap();

    // Left columns: 0.5 * 2 = 1, right columns: 0.25 * 2 = 0.5, all exact in 16-bit floats.
    assert_eq!(floats(&world, buffer), expected());
    assert_eq!(
        floats(&world, buffer).iter().filter(|v| **v != 0.0).count(),
        16,
        "every height is non-zero"
    );
}

#[test]
fn work_runs_in_the_order_it_was_called() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    let shader = scaling(&mut world);
    let (before, after) = (
        compute_buffer(&mut world, SIDE * SIDE),
        compute_buffer(&mut world, SIDE * SIDE),
    );
    // The first dispatch reads the height map before the camera draws it, the second after.
    dispatch(&mut world, shader, (heights, before), [2, 2, 1]);
    camera_render(&mut world, heights);
    dispatch(&mut world, shader, (heights, after), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    let written: Vec<_> = outcome.writes.iter().map(|write| write.id).collect();
    assert_eq!(written, [after], "the first buffer still holds zeros");
    outcome.apply(&mut world).unwrap();
    assert_eq!(floats(&world, before), vec![0.0; 16]);
    assert_eq!(floats(&world, after), expected());
}

#[test]
fn a_flush_with_no_work_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(outcome.writes.is_empty() && outcome.report.failures.is_empty());
}

/// A flush of four dispatches that cannot run, each for a different cause.
fn unrunnable_flush() -> FlushOutcome {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    write_compute(directory.path(), "text", vec![(1, text_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    // A shader whose file is not in the data directory, and one compiled for GLSL only.
    let unknown = compute_shader(&mut world, "missing");
    dispatch(&mut world, unknown, (heights, buffer), [2, 2, 1]);
    let text = compute_shader(&mut world, "text");
    dispatch(&mut world, text, (heights, buffer), [2, 2, 1]);
    // A kernel that names a texture the dispatch does not set.
    let bare = scaling(&mut world);
    world
        .bind_compute_buffer(bare, 0, "_Output".into(), buffer)
        .unwrap();
    world.dispatch_compute(bare, 0, [2, 2, 1]).unwrap();
    // A kernel with no buffer bound to the name it declares.
    let loose = scaling(&mut world);
    world
        .set_compute_property(
            loose,
            Some(0),
            PropertyValue::Texture(
                "_Heights".into(),
                super::support::texture_slot(
                    worldrunner_world_simulation::graphics::TextureRef::Render(heights),
                ),
            ),
        )
        .unwrap();
    world.dispatch_compute(loose, 0, [2, 2, 1]).unwrap();
    let snapshot = snapshot(&mut world);

    host.flush(&snapshot, &Quad).unwrap()
}

#[test]
fn a_dispatch_that_cannot_run_is_left_out_by_the_name_of_the_cause() {
    let outcome = unrunnable_flush();
    let failures: Vec<_> = outcome
        .report
        .failures
        .iter()
        .map(|(name, count)| (name.as_str(), *count))
        .collect();
    assert_eq!(
        failures,
        [
            ("dispatch:MissingTexture", 1),
            ("dispatch:NoCompiledCode", 1),
            ("dispatch:UnboundBuffer", 1),
            ("dispatch:Unreadable", 1),
        ]
    );
    assert!(outcome.writes.is_empty());
    assert!(outcome.writes.is_empty());
}

#[test]
fn the_trace_of_a_dispatch_that_cannot_run_names_the_cause_and_what_was_unset() {
    let outcome = unrunnable_flush();
    let outcomes: Vec<_> = outcome
        .report
        .traces
        .iter()
        .map(|trace| trace.outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "Unreadable",
            "NoCompiledCode",
            "MissingTexture",
            "UnboundBuffer"
        ]
    );
    assert_eq!(
        outcome.report.traces[2].textures[0].source, "unset",
        "the kernel declares a texture the dispatch never set"
    );
    assert!(outcome.report.traces.iter().all(|trace| !trace.read_back));
}

/// A flush of a camera render, a dispatch that reads it and the buffer it writes.
fn traced_flush() -> (FlushOutcome, ComputeBufferId) {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    (outcome, buffer)
}

#[test]
fn the_report_traces_what_a_dispatch_was_bound_to_and_what_came_back() {
    let (outcome, _) = traced_flush();
    let [trace] = outcome.report.traces.as_slice() else {
        panic!("one dispatch, one trace: {:?}", outcome.report.traces);
    };
    assert_eq!(
        (trace.shader.as_str(), trace.kernel.as_str()),
        ("heights", "Copy")
    );
    assert_eq!(
        trace.variant,
        "variant 0 of 1 for no keywords, thread group size [2, 2, 1]"
    );
    assert_eq!(
        (trace.groups, trace.thread_group),
        ([2, 2, 1], Some([2, 2, 1]))
    );
    assert_eq!(trace.constants, ["_Scale=[2.0, 0.0, 0.0, 0.0]"]);
    assert!(trace.constants_missing.is_empty());
    assert_eq!(trace.textures_set, ["_Heights"]);
    assert_eq!(trace.outcome, "ran");
    assert!(trace.read_back);
}

#[test]
fn the_trace_says_what_the_cameras_drew_into_the_texture_a_dispatch_reads() {
    let (outcome, _) = traced_flush();
    let [trace] = outcome.report.traces.as_slice() else {
        panic!("one dispatch, one trace");
    };
    let [texture] = trace.textures.as_slice() else {
        panic!("one texture: {:?}", trace.textures);
    };
    assert_eq!(texture.name, "_Heights");
    assert_eq!(texture.source, "render");
    assert!(texture.resolved);
    assert_eq!(
        (
            texture.camera_renders,
            texture.camera_draws,
            texture.camera_unmatched
        ),
        (1, 1, 0),
        "one camera drew one renderer into the height map before the dispatch"
    );
}

#[test]
fn the_trace_counts_the_non_zero_words_of_the_buffer_before_and_after() {
    let (outcome, buffer) = traced_flush();
    let [trace] = outcome.report.traces.as_slice() else {
        panic!("one dispatch, one trace");
    };
    let [output] = trace.buffers.as_slice() else {
        panic!("one buffer: {:?}", trace.buffers);
    };
    assert_eq!((output.name.as_str(), output.id), ("_Output", Some(buffer)));
    assert_eq!(output.bytes, 4 * (SIDE * SIDE) as usize);
    assert_eq!(output.nonzero_before, 0);
    assert_eq!(output.nonzero_after, Some(16));
}

#[test]
fn the_trace_reads_as_report_lines() {
    let (outcome, _) = traced_flush();
    let [trace] = outcome.report.traces.as_slice() else {
        panic!("one dispatch, one trace");
    };
    assert_eq!(
        trace.lines(),
        [
            "dispatch heights.Copy (variant 0 of 1 for no keywords, thread group size [2, 2, 1]) \
             groups [2, 2, 1], world thread group Some([2, 2, 1]): ran, read back true",
            "  constants set [_Scale=[2.0, 0.0, 0.0, 0.0]], without a value []",
            "  textures set [_Heights]",
            "  texture _Heights from render, on the GPU true, camera renders before 1 drew 1 left out 0 \
             for want of a pass, 0 for want of a texture and 0 for want of a mesh channel",
            "  buffer _Output 64 bytes, non-zero words 0 before, Some(16) after",
        ]
    );
}

#[test]
fn a_dispatch_before_any_camera_wrote_the_height_map_reads_back_zeros_and_says_so() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    camera_render(&mut world, heights);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert!(outcome.writes.is_empty(), "zeros over zeros change nothing");
    let [trace] = outcome.report.traces.as_slice() else {
        panic!("one dispatch, one trace");
    };
    assert_eq!(trace.textures[0].camera_renders, 0);
    assert_eq!(trace.buffers[0].nonzero_after, Some(0));
    assert_eq!(trace.outcome, "ran");
    assert!(trace.read_back);
}

/// The heights a read-back writes when the camera draws a shader whose passes have no
/// `LightMode`: the camera counts such a pass as `Alpha`, so it draws and the buffer is filled
/// with the same predicted values; a camera that does not leaves the height map at the clear
/// color and the draws counted.
#[test]
fn a_camera_draws_a_shader_without_a_light_mode_only_when_untagged_passes_count_as_its_value() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let settings = |untagged: Option<&str>| Settings {
        light_modes: vec!["Alpha".to_owned()],
        untagged_light_mode: untagged.map(str::to_owned),
    };
    let run = |untagged: Option<&str>| {
        let mut host = host_with(directory.path(), settings(untagged));
        let mut world = world();
        let heights = target(&mut world, HEIGHT);
        camera_render_of(&mut world, heights, "untagged");
        let shader = scaling(&mut world);
        let buffer = compute_buffer(&mut world, SIDE * SIDE);
        dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
        let snapshot = snapshot(&mut world);
        let outcome = host.flush(&snapshot, &Quad).unwrap();
        outcome.apply(&mut world).unwrap();
        (outcome, floats(&world, buffer))
    };

    let (outcome, heights) = run(Some("Alpha"));
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    assert_eq!(heights, expected());
    assert_eq!(outcome.report.traces[0].textures[0].camera_draws, 1);

    let (outcome, heights) = run(None);
    assert_eq!(
        outcome.report.failures.get("draw:NoPassForCamera"),
        Some(&1)
    );
    assert_eq!(
        heights,
        vec![CLEAR * SCALE; 16],
        "no draw: every texel is the clear color, scaled"
    );
    assert_eq!(outcome.report.traces[0].textures[0].camera_draws, 0);
    assert_eq!(outcome.report.traces[0].textures[0].camera_unmatched, 1);
}

/// A camera whose only draw samples `_Albedo`, which no layer names, leaves that draw out and
/// names the texture, and the dispatch trace says how many draws the height map missed.
#[test]
fn a_camera_draw_whose_texture_nothing_names_is_counted_by_the_texture_and_not_by_the_record() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let settings = Settings {
        light_modes: vec!["Gamma".to_owned()],
        ..Settings::default()
    };
    let mut host = host_with(directory.path(), settings);
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &Quad).unwrap();
    let report = &outcome.report;
    assert_eq!(report.failures.get("draw:MissingTexture"), Some(&1));
    assert_eq!(report.failures.get("record:MissingTexture"), None);
    assert_eq!(
        report.missing_textures,
        BTreeMap::from([("_Albedo".to_owned(), 1)])
    );
    assert_eq!(report.built, 1, "the camera render is kept");
    let texture = &report.traces[0].textures[0];
    assert_eq!(
        (
            texture.camera_renders,
            texture.camera_draws,
            texture.camera_missing_texture
        ),
        (1, 0, 1)
    );
    assert_eq!(texture.camera_unmatched, 0);
}

/// The heights a camera draws with `_Albedo` at the default its shader declares, which the
/// material leaves unset: the Gamma pass paints the albedo times the red tint of 0.5 over the
/// left half, so `white` gives 0.5 and `black` gives 0, each scaled by 2, over a clear color of
/// 0.25 scaled to 0.5.
fn heights_with_default(file: &str) -> (FlushOutcome, Vec<f32>) {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let settings = Settings {
        light_modes: vec!["Gamma".to_owned()],
        ..Settings::default()
    };
    let mut host = host_with(directory.path(), settings);
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render_of(&mut world, heights, file);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();
    (outcome, floats(&world, buffer))
}

#[test]
fn an_unset_texture_reads_the_white_its_shader_declares() {
    let (outcome, heights) = heights_with_default("default_white");
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    assert_eq!(heights, expected());
    assert_eq!(outcome.report.traces[0].textures[0].camera_draws, 1);
    assert_eq!(
        outcome.report.unresolved_textures,
        BTreeMap::from([(
            "_Albedo: no texture set, the shader's default read".to_owned(),
            1
        )])
    );
}

#[test]
fn an_unset_texture_reads_the_black_its_shader_declares() {
    let (outcome, heights) = heights_with_default("default_black");
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    let predicted: Vec<f32> = (0..SIDE * SIDE)
        .map(|index| {
            if index % SIDE < SIDE / 2 {
                0.0
            } else {
                CLEAR * SCALE
            }
        })
        .collect();
    assert_eq!(heights, predicted);
}

#[test]
fn an_unset_texture_whose_default_the_host_does_not_supply_leaves_the_draw_out() {
    let (outcome, heights) = heights_with_default("default_bump");
    assert_eq!(outcome.report.failures.get("draw:MissingTexture"), Some(&1));
    assert_eq!(
        outcome.report.missing_textures,
        BTreeMap::from([("_Albedo".to_owned(), 1)])
    );
    assert_eq!(heights, vec![CLEAR * SCALE; 16]);
}

/// A camera whose mesh lacks the texture coordinates its shader reads leaves that draw out and
/// names the channel and the input, and keeps the camera render.
#[test]
fn a_camera_draw_whose_mesh_lacks_a_channel_is_counted_by_the_channel_and_not_by_the_record() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    camera_render(&mut world, heights);
    let shader = scaling(&mut world);
    let buffer = compute_buffer(&mut world, SIDE * SIDE);
    dispatch(&mut world, shader, (heights, buffer), [2, 2, 1]);
    let snapshot = snapshot(&mut world);

    let outcome = host.flush(&snapshot, &BareQuad).unwrap();
    outcome.apply(&mut world).unwrap();
    let report = &outcome.report;
    assert_eq!(report.failures.get("draw:MissingChannel"), Some(&1));
    assert_eq!(report.failures.get("record:MissingChannel"), None);
    assert_eq!(
        report.missing_channels,
        BTreeMap::from([("channel 4 for input 1".to_owned(), 1)])
    );
    let texture = &report.traces[0].textures[0];
    assert_eq!(
        (texture.camera_draws, texture.camera_missing_channel),
        (0, 1)
    );
    assert_eq!(floats(&world, buffer), vec![CLEAR * SCALE; 16]);
}

#[test]
fn flushing_the_same_camera_render_again_holds_no_more_storage_than_the_first_flush_did() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let heights = target(&mut world, HEIGHT);
    let mut held = Vec::new();
    for _ in 0..4 {
        camera_render(&mut world, heights);
        let snapshot = snapshot(&mut world);
        let outcome = host.flush(&snapshot, &Quad).unwrap();
        assert!(
            outcome.report.failures.is_empty(),
            "{:?}",
            outcome.report.failures
        );
        held.push(host.live_bytes());
    }
    assert!(held[0] > 0, "the first flush holds the height map");
    assert_eq!(
        held,
        vec![held[0]; 4],
        "the render texture and the constants buffer are reused: {held:?}"
    );
}

#[test]
fn a_render_texture_the_world_destroyed_gives_its_storage_back_at_the_next_flush() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    let mut held = Vec::new();
    let mut previous = None;
    for _ in 0..4 {
        // A temporary: each flush draws into a new render texture and the script destroys it
        // when the next frame begins.
        let heights = target(&mut world, HEIGHT);
        camera_render(&mut world, heights);
        if let Some(old) = previous.replace(heights) {
            world.destroy_render_texture(old).unwrap();
        }
        let snapshot = snapshot(&mut world);
        let outcome = host.flush(&snapshot, &Quad).unwrap();
        assert!(
            outcome.report.failures.is_empty(),
            "{:?}",
            outcome.report.failures
        );
        held.push(host.live_bytes());
    }
    // The destroyed texture goes back before the new one is made, and a finished flush leaves
    // nothing released but still counted, so every flush ends holding one render texture.
    assert!(held[0] > 0, "{held:?}");
    assert_eq!(held, vec![held[0]; 4], "{held:?}");
}

#[test]
fn a_render_texture_too_big_for_the_budget_is_dropped_and_the_rest_of_the_frame_runs() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    // The runtime may hold 64 MiB and this render texture alone is 64 MiB of RGBA8.
    let huge = render_texture(&mut world, [4096, 4096]);
    camera_render(&mut world, huge);
    let small = target(&mut world, HEIGHT);
    camera_render(&mut world, small);
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert_eq!(
        outcome.report.failures.get("rendertexture:Program"),
        Some(&1)
    );
    assert_eq!(
        outcome.report.failures.get("record:UnusableRenderTexture"),
        Some(&1)
    );
    let traces = host.take_traces();
    let trace = traces.last().unwrap();
    assert_eq!((trace.records, trace.built), (2, 1));
    // One whole-frame refusal, then each of the two render textures built alone. The record that
    // remains is not built alone: no record allocates storage, so a budget cannot be its fault.
    assert_eq!(
        (trace.phases.refusals, trace.phases.probes),
        (1, 2),
        "{trace:?}"
    );
}

#[test]
fn render_textures_that_each_fit_the_budget_but_not_together_keep_the_first_and_search_no_record() {
    let directory = tempfile::tempdir().unwrap();
    write_compute(directory.path(), "heights", vec![(1, copy_object())]);
    let mut host = host(directory.path());
    let mut world = world();
    // Three render textures of 36 MB each: any one fits the 64 MiB budget and no two do.
    for _ in 0..3 {
        let part = render_texture(&mut world, [3000, 3000]);
        camera_render(&mut world, part);
    }
    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    assert_eq!(
        outcome.report.failures.get("rendertexture:Program"),
        Some(&2)
    );
    let traces = host.take_traces();
    let trace = traces.last().unwrap();
    assert_eq!((trace.records, trace.built), (3, 1));
    // The whole frame was refused once and the three render textures were built alone; none of
    // the records was, since a budget is no record's fault.
    assert_eq!(
        (trace.phases.refusals, trace.phases.probes),
        (1, 3),
        "{trace:?}"
    );
}
