//! Persistent targets: kept across frames, left alone by cancelled frames, kept across renderer
//! replacement, and steady frames that build nothing.
use super::fixture::{FORMAT, Rig, SIZE, quad_scene};
use super::layers::{self, ARRAY, HISTORY, LAYERS, LayersGame, READER, Shared};
use std::sync::Arc;
use worldrunner_render_gpu::tolerance_test_support::{assert_code_within, encoded_steps};
use worldrunner_render_scene::{
    compositor::GamePasses,
    executor::{FrameConfig, FrameError},
    wgpu,
};

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const HALF_BLUE: [f32; 4] = [0.0, 0.0, 128.0 / 255.0, 1.0];

fn rig() -> (Rig, Shared, GamePasses) {
    let knobs = layers::knobs();
    let passes = GamePasses::new().with(Arc::new(LayersGame {
        knobs: knobs.clone(),
    }));
    let inventory = passes.clone();
    let rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    (rig, knobs, passes)
}

fn fill_all(knobs: &Shared) {
    knobs.lock().unwrap().produce = vec![(0, RED), (1, GREEN), (2, HALF_BLUE)];
}

/// The reader writes the red of layer 0, the green of layer 1 and the blue of layer 2 over the
/// whole frame; a pixel outside the stock quad shows exactly that.
fn assert_filled(image: &[[u8; 4]], context: &str) {
    let pixel = Rig::pixel(image, 3, 20);
    assert_code_within(
        pixel[0],
        encoded_steps(1.0, true),
        0.0,
        &format!("{context} red"),
    );
    assert_code_within(
        pixel[1],
        encoded_steps(1.0, true),
        0.0,
        &format!("{context} green"),
    );
    assert_code_within(
        pixel[2],
        encoded_steps(128.0 / 255.0, true),
        0.0,
        &format!("{context} blue"),
    );
    assert_eq!(pixel[3], 255, "{context} alpha");
}

#[test]
fn a_persistent_array_filled_in_one_frame_reads_unchanged_in_the_next_without_being_rewritten() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, knobs, passes) = rig();
    fill_all(&knobs);
    assert_filled(&rig.render().unwrap(), "frame 1");
    assert!(passes.targets().is_written(ARRAY, 2));
    {
        let mut knobs = knobs.lock().unwrap();
        knobs.produce.clear();
        knobs.counts.recorded.clear();
    }
    assert_filled(&rig.render().unwrap(), "frame 2");
    assert_eq!(
        knobs.lock().unwrap().counts.recorded,
        vec![READER],
        "frame 2 recorded no producer"
    );
}

#[test]
fn a_cancelled_frame_copies_no_constants_and_leaves_persistent_targets_unchanged() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, knobs, passes) = rig();
    // A cancelled first frame writes nothing the targets or their bookkeeping keep.
    fill_all(&knobs);
    rig.render_cancelled().unwrap();
    let constants = knobs.lock().unwrap().constants.clone().unwrap();
    assert_eq!(
        rig.read_buffer(&constants),
        vec![0; 16],
        "no constants were copied"
    );
    assert!(!passes.targets().is_written(ARRAY, 0));
    knobs.lock().unwrap().produce.clear();
    assert!(
        matches!(rig.render(), Err(FrameError::MissingProducer(resource)) if resource.name == ARRAY),
        "the cancelled frame left the array unwritten"
    );
    // A cancelled frame after a submitted one leaves its constants and its layers.
    fill_all(&knobs);
    rig.render().unwrap();
    knobs.lock().unwrap().produce = vec![(0, GREEN)];
    rig.render_cancelled().unwrap();
    let kept: Vec<u8> = RED.iter().flat_map(|value| value.to_le_bytes()).collect();
    assert_eq!(
        rig.read_buffer(&constants),
        kept,
        "the cancelled upload was never copied"
    );
    knobs.lock().unwrap().produce.clear();
    assert_filled(&rig.render().unwrap(), "after the cancelled frame");
}

#[test]
fn renderer_replacement_registers_the_factory_again_and_keeps_fixed_persistent_targets() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, knobs, passes) = rig();
    fill_all(&knobs);
    assert_filled(&rig.render().unwrap(), "before replacement");
    let array = passes.targets().texture(ARRAY).unwrap();
    let created = passes.targets().created();
    assert_eq!(knobs.lock().unwrap().counts.builds, 1);

    let mut settings = rig.renderer.configuration().clone();
    settings.clear_color = wgpu::Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    rig.renderer
        .reconfigure(&rig.context, settings.clone())
        .unwrap();
    assert_eq!(
        knobs.lock().unwrap().counts.builds,
        2,
        "the factory built again"
    );
    assert_eq!(
        passes.targets().created(),
        created,
        "no target was recreated"
    );
    assert_eq!(passes.targets().texture(ARRAY).unwrap(), array);
    knobs.lock().unwrap().produce.clear();
    assert_filled(&rig.render().unwrap(), "after replacement");

    // A frame-sized persistent target is recreated by a resize, which discards its contents;
    // the fixed-size array keeps its texture and its layers.
    settings.frame = FrameConfig::new([32, 24], FORMAT, 1);
    rig.renderer.reconfigure(&rig.context, settings).unwrap();
    assert_eq!(
        passes.targets().created(),
        created + 1,
        "only the history target"
    );
    assert_eq!(passes.targets().texture(ARRAY).unwrap(), array);
    assert!(passes.targets().is_written(ARRAY, LAYERS - 1));
    assert!(!passes.targets().is_written(HISTORY, 0));
}

#[test]
fn steady_state_frames_build_nothing_new() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, knobs, passes) = rig();
    fill_all(&knobs);
    for _ in 0..3 {
        rig.render().unwrap();
    }
    let stats = knobs.lock().unwrap().stats.unwrap();
    let created = passes.targets().created();
    for _ in 0..3 {
        assert_filled(&rig.render().unwrap(), "steady");
    }
    assert_eq!(
        knobs.lock().unwrap().stats.unwrap(),
        stats,
        "program runtime built nothing"
    );
    assert_eq!(
        passes.targets().created(),
        created,
        "no target was allocated"
    );
}
