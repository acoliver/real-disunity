//! Every allocation the runtime makes is admitted against the budget it was given.

use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{ProgramRuntime, TargetHandle};

use crate::common::{Gpu, run, submit};

const RGBA8: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// A 4x4 Rgba8 color texture.
const TARGET_BYTES: u64 = 4 * 4 * 4;
/// The copy of that texture: each 16-byte row pads to 256, for four rows.
const TARGET_READBACK_BYTES: u64 = 256 * 4;

fn target(runtime: &mut ProgramRuntime, depth: bool) -> Result<TargetHandle, ProgramError> {
    runtime.create_render_target("budget", [4, 4], RGBA8, depth.then_some(DEPTH))
}

fn over(required: u64, budget: u64) -> ProgramError {
    ProgramError::OverBudget { required, budget }
}

#[test]
fn a_target_over_budget_is_refused_by_name_and_nothing_is_created() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(TARGET_BYTES / 2));
    assert_eq!(
        target(&mut runtime, false).unwrap_err(),
        over(TARGET_BYTES, TARGET_BYTES / 2)
    );
    assert_eq!(runtime.live_bytes(), 0);
    // The refusal reserved nothing, so a texture that fits is still admitted.
    runtime
        .create_texture_2d("small", [2, 2], RGBA8, &[0; 16])
        .unwrap();
    assert_eq!(runtime.live_bytes(), 16);
}

#[test]
fn a_depth_attachment_counts_against_the_budget() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(2 * TARGET_BYTES - 1));
    assert_eq!(
        target(&mut runtime, true).unwrap_err(),
        over(2 * TARGET_BYTES, 2 * TARGET_BYTES - 1)
    );
    let mut exact = gpu.runtime_with(AllocationBudget(2 * TARGET_BYTES));
    target(&mut exact, true).unwrap();
    assert_eq!(exact.live_bytes(), 2 * TARGET_BYTES);
}

#[test]
fn a_buffer_over_budget_is_refused_by_name() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(16));
    // Twenty bytes: buffer copies need a multiple of four, and 20 already is one.
    assert_eq!(
        runtime.create_buffer("big", &[0; 20]).unwrap_err(),
        over(20, 16)
    );
    assert_eq!(runtime.live_bytes(), 0);
    // Three bytes pad to four.
    runtime.create_buffer("small", &[0; 3]).unwrap();
    assert_eq!(runtime.live_bytes(), 4);
}

#[test]
fn a_texture_over_budget_is_refused_before_it_is_uploaded() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(8));
    assert_eq!(
        runtime
            .create_texture_2d("t", [2, 2], RGBA8, &[0; 16])
            .unwrap_err(),
        over(16, 8)
    );
    assert_eq!(runtime.live_bytes(), 0);
}

#[test]
fn releasing_a_resource_admits_the_next_one() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(2 * TARGET_BYTES));
    let first = target(&mut runtime, false).unwrap();
    target(&mut runtime, false).unwrap();
    assert_eq!(
        target(&mut runtime, false).unwrap_err(),
        over(3 * TARGET_BYTES, 2 * TARGET_BYTES)
    );
    assert!(runtime.release_target(first).unwrap());
    target(&mut runtime, false).unwrap();
    assert_eq!(runtime.live_bytes(), 2 * TARGET_BYTES);
    assert!(!runtime.release_target(first).unwrap(), "already released");
}

#[test]
fn bytes_of_a_released_resource_return_once_the_work_that_used_it_has_finished() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(TARGET_BYTES));
    let used = target(&mut runtime, false).unwrap();
    run(&mut runtime, |runtime, encoder| {
        runtime.clear_target(encoder, used, [1.0; 4], 1.0)
    })
    .unwrap();
    assert!(runtime.release_target(used).unwrap());
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    runtime.collect();
    assert_eq!(runtime.live_bytes(), 0);
    target(&mut runtime, false).unwrap();
}

#[test]
fn releasing_while_a_frame_is_being_recorded_is_busy() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let buffer = runtime.create_buffer("held", &[0; 4]).unwrap();
    let frame = gpu.authority.begin(&gpu.device).unwrap();
    assert_eq!(
        runtime.release_buffer(buffer).unwrap_err(),
        ProgramError::Busy
    );
    drop(frame);
    assert!(runtime.release_buffer(buffer).unwrap());
}

#[test]
fn a_pending_readback_holds_its_staging_bytes_until_it_is_resolved_or_dropped_and_its_frame_completed()
 {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(TARGET_BYTES + TARGET_READBACK_BYTES));
    let handle = target(&mut runtime, false).unwrap();
    let (first, index) = submit(&mut runtime, |runtime, encoder| {
        runtime.read_target(encoder, handle)
    })
    .unwrap();
    assert_eq!(runtime.live_bytes(), TARGET_BYTES + TARGET_READBACK_BYTES);
    let refused = submit(&mut runtime, |runtime, encoder| {
        runtime.read_target(encoder, handle)
    });
    assert_eq!(
        refused.err(),
        Some(over(
            TARGET_BYTES + 2 * TARGET_READBACK_BYTES,
            TARGET_BYTES + TARGET_READBACK_BYTES
        ))
    );
    first.resolve(index).unwrap();
    assert_eq!(runtime.live_bytes(), TARGET_BYTES);

    let (dropped, _) = submit(&mut runtime, |runtime, encoder| {
        runtime.read_target(encoder, handle)
    })
    .unwrap();
    drop(dropped);
    // The frame that recorded the copy holds the staging until the GPU has completed it.
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_eq!(runtime.live_bytes(), TARGET_BYTES);
}

#[test]
fn the_staging_of_a_recorded_write_is_admitted_against_the_budget() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime_with(AllocationBudget(16));
    let buffer = runtime.create_buffer("full", &[0; 16]).unwrap();
    let refused = submit(&mut runtime, |runtime, encoder| {
        runtime.write_buffer(encoder, buffer, 0, &[1, 2, 3, 4])
    });
    assert_eq!(refused.err(), Some(over(20, 16)));
    assert_eq!(runtime.live_bytes(), 16, "a refused write holds nothing");
}
