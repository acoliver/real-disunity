//! Persistent targets: kept across frames, left alone by cancelled frames, kept across renderer
//! replacement, and steady frames that build nothing.
use super::fixture::{Rig, SCENE_FORMAT, SIZE, quad_scene};
use super::layers::{self, ARRAY, HISTORY, HISTORY_PRODUCER, LAYERS, LayersGame, READER, Shared};
use super::prepared;
use std::sync::Arc;
use worldrunner_render_gpu::tolerance_test_support::{assert_code_within, encoded_steps};
use worldrunner_render_post::{FrameConfig, FrameError};
use worldrunner_render_scene::{compositor::GamePasses, wgpu};

pub const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
pub const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
pub const HALF_BLUE: [f32; 4] = [0.0, 0.0, 128.0 / 255.0, 1.0];

pub fn rig() -> (Rig, Shared) {
    let knobs = layers::knobs();
    let inventory = GamePasses::new().with(Arc::new(LayersGame {
        knobs: knobs.clone(),
    }));
    let rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = inventory;
    });
    (rig, knobs)
}

/// Every layer of the array and the history target, written in one frame.
pub fn fill_all(knobs: &Shared) {
    knobs.lock().unwrap().produce = vec![
        (0, RED),
        (1, GREEN),
        (2, HALF_BLUE),
        (HISTORY_PRODUCER, RED),
    ];
}

/// The reader writes the red of layer 0, the green of layer 1 and the blue of layer 2 over the
/// whole frame; a pixel outside the stock quad shows exactly that.
pub fn assert_filled(image: &[[u8; 4]], context: &str) {
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
    let (mut rig, knobs) = rig();
    fill_all(&knobs);
    assert_filled(&rig.render().unwrap(), "frame 1");
    assert!(rig.renderer.game_targets().is_written(ARRAY, 2));
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
    let (mut rig, knobs) = rig();
    // A cancelled first frame writes nothing the targets or their bookkeeping keep.
    fill_all(&knobs);
    rig.render_cancelled().unwrap();
    let constants = knobs.lock().unwrap().constants.clone().unwrap();
    assert_eq!(
        rig.read_buffer(&constants),
        vec![0; 16],
        "no constants were copied"
    );
    assert!(!rig.renderer.game_targets().is_written(ARRAY, 0));
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
    let (mut rig, knobs) = rig();
    fill_all(&knobs);
    assert_filled(&rig.render().unwrap(), "before replacement");
    let array = rig.renderer.game_targets().texture(ARRAY).unwrap();
    let created = rig.renderer.game_targets().created();
    let views = rig.renderer.game_targets().views_created();
    assert!(
        rig.renderer.game_targets().is_written(HISTORY, 0),
        "the frame wrote the history target"
    );
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
        rig.renderer.game_targets().created(),
        created,
        "no target was recreated"
    );
    assert_eq!(rig.renderer.game_targets().texture(ARRAY).unwrap(), array);
    knobs.lock().unwrap().produce.clear();
    assert_filled(&rig.render().unwrap(), "after replacement");
    assert_eq!(
        rig.renderer.game_targets().views_created(),
        views,
        "the kept textures kept their views"
    );

    // A frame-sized persistent target is recreated by a resize, which discards its contents;
    // the fixed-size array keeps its texture and its layers.
    settings.frame = FrameConfig::new([32, 24], SCENE_FORMAT, 1);
    rig.renderer.reconfigure(&rig.context, settings).unwrap();
    assert_eq!(
        rig.renderer.game_targets().created(),
        created + 1,
        "only the history target"
    );
    assert_eq!(rig.renderer.game_targets().texture(ARRAY).unwrap(), array);
    assert!(rig.renderer.game_targets().is_written(ARRAY, LAYERS - 1));
    assert!(!rig.renderer.game_targets().is_written(HISTORY, 0));
    knobs.lock().unwrap().produce = vec![(HISTORY_PRODUCER, RED)];
    rig.render().unwrap();
    let resized = rig.renderer.game_targets().views_created();
    assert!(
        resized > views,
        "the recreated history target got views of its new texture"
    );
    rig.render().unwrap();
    assert_eq!(
        rig.renderer.game_targets().views_created(),
        resized,
        "and kept them"
    );
}

#[test]
fn steady_state_frames_build_nothing_new() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, knobs) = rig();
    fill_all(&knobs);
    for _ in 0..3 {
        rig.render().unwrap();
    }
    let stats = knobs.lock().unwrap().stats.unwrap();
    let created = rig.renderer.game_targets().created();
    let views = rig.renderer.game_targets().views_created();
    assert!(views > 0, "the frames attached views of the targets");
    for _ in 0..3 {
        assert_filled(&rig.render().unwrap(), "steady");
    }
    assert_eq!(
        rig.renderer.game_targets().views_created(),
        views,
        "steady frames made no view of a target"
    );
    assert_eq!(
        knobs.lock().unwrap().stats.unwrap(),
        stats,
        "program runtime built nothing"
    );
    assert_eq!(
        rig.renderer.game_targets().created(),
        created,
        "no target was allocated"
    );
}

#[test]
fn a_cancelled_frame_with_a_prepared_write_leaves_the_layer_unwritten() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let knobs = prepared::knobs(&[0, 1], &[0, 1]);
    let mut rig = prepared::rig(&knobs);
    rig.render_cancelled().unwrap();
    assert!(!rig.renderer.game_targets().is_written(prepared::BLOCKS, 0));
    prepared::next(&knobs, &[], &[0]);
    assert!(
        matches!(rig.render(), Err(FrameError::MissingProducer(resource)) if resource.name == prepared::BLOCKS),
        "the cancelled frame left the blocks unwritten"
    );
    prepared::next(&knobs, &[0, 1], &[0, 1]);
    prepared::assert_blocks(&rig.render().unwrap(), true, "compressed again");
    assert!(rig.renderer.game_targets().is_written(prepared::BLOCKS, 1));
}
