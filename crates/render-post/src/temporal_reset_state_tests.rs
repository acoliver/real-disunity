//! Reset equivalence of the temporal chain with the reconstruction and depth clip stages
//! present, on lavapipe. The warmed chain has accumulated history over frames with a moving bar,
//! real depth, residual motion and a camera with depth terms, so the dilated depth and motion,
//! the scatter buffer, the confidence plane and the history all hold state. The first output
//! after each reset cause must equal the first output of a fresh chain built at the same extents
//! and given the same inputs. Both runs execute the same code on one device, so the comparison
//! is exact (the half float values read back are equal).
use crate::taa_reconstruct_test_rig::static_camera;
use crate::temporal_disocclusion_scene as scene;
use crate::temporal_scene_rig::{OUTPUT, SceneRig};
use crate::{TemporalCamera, gpu_tests};

const RENDER: [u32; 2] = [80, 48];
/// Frames of history before the reset frame.
const WARM: u64 = 8;

type Gpu = (wgpu::Device, wgpu::Queue);

#[expect(
    clippy::cast_possible_truncation,
    reason = "the bar's left edge and velocity are small canvas coordinates"
)]
fn bar(index: u64) -> [f32; 4] {
    [scene::bar_left(index) as f32, scene::SPEED as f32, 0.0, 0.0]
}

fn frame(rig: &SceneRig, index: u64, history_valid: bool, camera: TemporalCamera) {
    rig.frame(index, history_valid, (bar(index), Some(camera)));
}

fn warmed(gpu: &Gpu) -> SceneRig {
    let rig = SceneRig::new(gpu, RENDER, scene::wgsl());
    for index in 0..WARM {
        frame(&rig, index, index > 0, static_camera());
    }
    rig
}

/// The reset frame: the next index, no history.
fn reset_frame(rig: &SceneRig, camera: TemporalCamera) -> Vec<f64> {
    frame(rig, WARM, false, camera);
    rig.image()
}

fn fresh(gpu: &Gpu, render: [u32; 2], output: [u32; 2], camera: TemporalCamera) -> Vec<f64> {
    let rig = SceneRig::with_output(gpu, render, output, scene::wgsl());
    reset_frame(&rig, camera)
}

#[test]
fn history_changes_the_output_so_the_reset_comparisons_are_not_vacuous() {
    let gpu = gpu_tests::gpu();
    let rig = warmed(&gpu);
    frame(&rig, WARM, true, static_camera());
    let continued = rig.image();
    assert_ne!(continued, fresh(&gpu, RENDER, OUTPUT, static_camera()));
}

#[test]
fn a_reset_in_place_matches_a_fresh_chain() {
    let gpu = gpu_tests::gpu();
    let rig = warmed(&gpu);
    assert_eq!(
        reset_frame(&rig, static_camera()),
        fresh(&gpu, RENDER, OUTPUT, static_camera())
    );
}

#[test]
fn a_cut_without_a_camera_matches_a_fresh_chain() {
    let gpu = gpu_tests::gpu();
    let rig = warmed(&gpu);
    assert_eq!(
        reset_frame(&rig, TemporalCamera::NONE),
        fresh(&gpu, RENDER, OUTPUT, TemporalCamera::NONE)
    );
}

#[test]
fn a_reset_after_a_cancelled_output_matches_a_fresh_chain() {
    let gpu = gpu_tests::gpu();
    let rig = warmed(&gpu);
    // A frame recorded and dropped without submission leaves no trace in the chain.
    rig.cancelled_frame(WARM, true, bar(WARM));
    assert_eq!(
        reset_frame(&rig, static_camera()),
        fresh(&gpu, RENDER, OUTPUT, static_camera())
    );
}

#[test]
fn a_frame_after_a_cancelled_output_continues_the_committed_history() {
    let gpu = gpu_tests::gpu();
    let cancelled = warmed(&gpu);
    cancelled.cancelled_frame(WARM, true, bar(WARM));
    frame(&cancelled, WARM, true, static_camera());
    let plain = warmed(&gpu);
    frame(&plain, WARM, true, static_camera());
    assert_eq!(cancelled.image(), plain.image());
}

fn assert_resize_resets(render: [u32; 2], output: [u32; 2]) {
    let gpu = gpu_tests::gpu();
    let mut rig = warmed(&gpu);
    rig.resize(render, output);
    assert_eq!(
        reset_frame(&rig, static_camera()),
        fresh(&gpu, render, output, static_camera())
    );
}

#[test]
fn a_reset_after_a_render_only_resize_matches_a_fresh_chain() {
    assert_resize_resets([107, 64], OUTPUT);
}

#[test]
fn a_reset_after_an_output_only_resize_matches_a_fresh_chain() {
    assert_resize_resets(RENDER, [150, 90]);
}

#[test]
fn a_reset_after_resizing_both_extents_matches_a_fresh_chain() {
    assert_resize_resets([123, 74], [200, 120]);
}
