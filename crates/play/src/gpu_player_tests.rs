use super::*;

use crate::retained_driver::ui_tests::{booted_fixture_host, fixture_turn};

fn trace(records: usize, error: Option<&str>) -> WorkTrace {
    WorkTrace {
        records,
        error: error.map(str::to_owned),
        ..WorkTrace::default()
    }
}

fn listing(names: &[&str]) -> Report {
    let mut report = Report::default();
    for name in names {
        report.global_textures.insert((*name).to_owned(), 1);
    }
    report
}

#[test]
fn each_piece_of_work_lists_the_global_textures_it_saw_with_the_frame_it_belongs_to() {
    let mut report = GpuWorkReport::default();
    report.add(&listing(&["_Height: render"]), "flush");
    report.add(&Report::default(), "flush");
    report.frames = 3;
    report.add(&listing(&["_Height: render", "_Last: none"]), "flush");
    report.add(&listing(&["_Last: render"]), "frame");
    let found: Vec<_> = report
        .global_listings
        .iter()
        .map(GlobalListing::line)
        .collect();
    assert_eq!(
        found,
        [
            "globals at a flush of frame 0: _Height: render",
            "globals at a flush of frame 3: _Height: render, _Last: none",
            "globals at a frame of frame 3: _Last: render",
        ]
    );
}

#[test]
fn work_the_gpu_refused_whole_is_counted_with_its_records_and_the_rest_is_not() {
    let mut report = GpuWorkReport::default();
    report.keep_traces(vec![
        trace(2513, Some("over its budget")),
        trace(27, None),
        trace(30, Some("a shader")),
    ]);
    assert_eq!((report.refused_works, report.refused_records), (2, 2543));
    assert_eq!(report.work.len(), 3, "every piece of work keeps its trace");
}

/// The limits an adapter that offers `storage_images` storage images in a stage and buffers
/// of `adapter_buffer` bytes would give the player's request for a budget of `budget`.
fn negotiated(storage_images: u32, adapter_buffer: u64, budget: u64) -> wgpu::Limits {
    let available = wgpu::Limits {
        max_buffer_size: adapter_buffer,
        max_storage_textures_per_shader_stage: storage_images,
        ..wgpu::Limits::default()
    };
    worldrunner_render_gpu::device::negotiate(
        wgpu::Features::empty(),
        &available,
        &device_requirements(budget, &available),
    )
    .unwrap()
    .limits
}

#[test]
fn the_headless_device_asks_for_the_storage_images_the_old_world_kernels_bind_when_offered() {
    let budget = storage_budget::DEFAULT_BYTES;
    // Two Old World compute variants bind five storage images in their one stage, one more
    // than the default limit of four.
    let offered = negotiated(16, budget * 2, budget);
    assert!(offered.max_storage_textures_per_shader_stage >= 5);
    assert_eq!(offered.max_buffer_size, budget);
    assert_eq!(
        negotiated(4, budget * 2, budget).max_storage_textures_per_shader_stage,
        4,
        "an adapter at the default limit still gets a device"
    );
}

#[test]
fn an_adapter_whose_buffers_are_smaller_than_the_budget_still_gets_a_device() {
    // The budget is a total, not a buffer: the device takes the adapter's largest buffer.
    let default = wgpu::Limits::default().max_buffer_size;
    let limits = negotiated(16, default, 4 * default);
    assert_eq!(limits.max_buffer_size, default);
    assert!(limits.max_storage_textures_per_shader_stage >= 5);
}

#[test]
fn the_headless_device_takes_the_budget_the_chooser_gives_its_adapter() {
    let directory = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(directory.path()).unwrap();
    assert!(gpu.storage_budget() >= storage_budget::DEFAULT_BYTES);
    assert!(
        gpu.report().device.contains("runtime budget"),
        "the report says where the budget came from"
    );
}

/// Plays the portable screen fixture on `gpu` for `frames` frames. Its script clears the screen to
/// yellow in its first two updates: the one while the scene loads and the first the host advances.
fn play_screen_fixture(gpu: &PlayerGpu, frames: u64) {
    let mut host = booted_fixture_host();
    gpu.install(&mut host).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture/screen.fixture"))
        .unwrap();
    for _ in 0..frames {
        host.advance(16_000_000, 1_000_000_000).unwrap();
        gpu.frame(&mut host).unwrap();
    }
}

#[test]
fn a_frame_that_clears_the_screen_gives_its_image_and_frames_after_it_leave_the_image_alone() {
    let _turn = fixture_turn();
    let data = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(data.path()).unwrap();
    gpu.set_screen(Some([8, 4]));
    assert!(gpu.screen().is_none(), "nothing drew yet");

    play_screen_fixture(&gpu, 4);

    let image = gpu.screen().expect("the clear was read back");
    assert_eq!((image.width, image.height), (8, 4));
    assert_eq!(image.frame, 0, "both clears were in the first frame's work");
    assert!(
        image
            .rgba
            .chunks_exact(4)
            .all(|texel| texel == [255, 255, 0, 255]),
        "the screen is yellow all over"
    );
    let report = gpu.report();
    assert_eq!((report.frames, report.screen_reads), (4, 1));
    assert!(report.screen_reads_refused.is_empty());
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(
        report
            .screen_line()
            .starts_with("[gpu] screen images read back 1, refused 0")
    );
}

#[test]
fn without_a_screen_the_clear_is_left_out_and_no_image_exists() {
    let _turn = fixture_turn();
    let data = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(data.path()).unwrap();

    play_screen_fixture(&gpu, 2);

    assert!(gpu.screen().is_none());
    let report = gpu.report();
    assert_eq!(report.screen_reads, 0);
    assert!(
        report
            .skipped_by_kind
            .contains_key("SetRenderTarget: UnboundTarget"),
        "{:?}",
        report.skipped_by_kind
    );
}

#[test]
fn a_screen_readback_that_was_refused_is_counted_by_its_reason() {
    let report = GpuWorkReport {
        screen_reads: 3,
        screen_reads_refused: BTreeMap::from([("over the storage budget".to_owned(), 2)]),
        ..GpuWorkReport::default()
    };
    assert_eq!(
        report.screen_line(),
        "[gpu] screen images read back 3, refused 2 (over the storage budget: 2)"
    );
}
