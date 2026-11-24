use super::*;

use crate::gpu_device::device_requirements;
use crate::gpu_device::{DeviceSource, PlayerStartupInputs, SharedDevice, headless};
use crate::offscreen::Settling;
use crate::player::window_gpu;
use crate::retained_driver::ui_tests::{booted_fixture_host, fixture_turn};
use crate::storage_budget;
use worldrunner_render_gpu::device::DeviceRequirements;
use worldrunner_render_program::offscreen::PendingOffscreen;
use worldrunner_render_program::unity::offscreen::ViewCounts;
use worldrunner_scene_viewer::SharedGpu;

fn trace(records: usize, error: Option<&str>) -> WorkTrace {
    WorkTrace {
        records,
        error: error.map(str::to_owned),
        ..WorkTrace::default()
    }
}

fn names(entries: &[&str]) -> BTreeSet<String> {
    entries.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn the_run_report_keeps_each_unset_constant_once_over_every_piece_of_work() {
    let mut report = GpuWorkReport::default();
    let first = Report {
        unset_constants: names(&["_B", "_A"]),
        ..Report::default()
    };
    let second = Report {
        unset_constants: names(&["_A", "_C"]),
        ..Report::default()
    };
    report.add(&first, "frame");
    report.add(&second, "flush");
    assert_eq!(report.unset_constants, names(&["_A", "_B", "_C"]));
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
fn the_run_report_totals_the_planner_counts_of_every_piece_of_work() {
    let views = |created, reused| ViewCounts { created, reused };
    let mut first = trace(1, None);
    first.phases.note_build(5, views(2, 7));
    first.phases.note_build(9, views(1, 0));
    let mut second = trace(1, Some("over its budget"));
    second.phases.note_build(4, views(3, 4));
    let mut report = GpuWorkReport::default();
    report.keep_traces(vec![first, second]);
    assert_eq!(
        report.planner.planner_line(),
        "builds 3, most draws held in one build 9, texture views made 6 and reused 11"
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

#[test]
fn the_first_work_the_gpu_refused_whole_is_what_the_run_is_told_of_once() {
    let mut report = GpuWorkReport::default();
    let flush = WorkTrace {
        kind: "flush",
        ..trace(3, Some("over its budget of 7"))
    };
    report.keep_traces(vec![trace(1, None), flush, trace(2, Some("a shader"))]);
    assert_eq!(
        report.refusal.take().as_deref(),
        Some("flush refused by the GPU: over its budget of 7")
    );
    assert_eq!(report.refusal, None);
}

#[test]
fn a_render_texture_the_budget_refused_is_a_refusal_and_a_refused_upload_is_not() {
    let mut unsupported = Report::default();
    for cause in [
        "texture:Program/Layout/UnsupportedParameter",
        "texture:Program/OverBudget",
        "mesh:Program/OverBudget",
    ] {
        unsupported.causes.insert(cause.to_owned(), 1);
    }
    let mut over = Report::default();
    let cause = "rendertexture:Program/OverBudget";
    over.causes.insert(cause.to_owned(), 2);
    over.examples
        .insert(cause.to_owned(), "the allocation takes 9 bytes".to_owned());
    let mut report = GpuWorkReport::default();
    report.add(&unsupported, "frame");
    assert_eq!(report.refusal, None);
    report.add(&over, "flush");
    assert_eq!(
        report.refusal.as_deref(),
        Some(
            "flush refused by the GPU: rendertexture:Program/OverBudget: the allocation takes 9 bytes"
        )
    );
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
fn the_headless_device_is_granted_the_storage_images_the_scene_device_is() {
    // The window's device is the scene's, which takes the adapter's count up to sixteen.
    let budget = storage_budget::DEFAULT_BYTES;
    for (offered, expected) in [(1 << 20, 16), (32, 16), (16, 16), (8, 8), (5, 5), (4, 4)] {
        assert_eq!(
            negotiated(offered, budget * 2, budget).max_storage_textures_per_shader_stage,
            expected,
            "an adapter offering {offered}"
        );
    }
}

/// A launch that supplies `value` and asks for nothing else.
fn launch_with(value: SuppliedGlobal) -> GpuLaunch {
    GpuLaunch {
        globals: vec![value],
        ..GpuLaunch::default()
    }
}

#[test]
fn the_launch_sets_the_globals_and_the_stand_in_look_of_the_offscreen_settings() {
    let value = SuppliedGlobal::parse("unity_LightData=1,1,1,1").unwrap();
    let launch = GpuLaunch {
        stand_ins: StandInLook::Magenta,
        ..launch_with(value)
    };
    let settings = launch.settings();
    assert_eq!(settings.stand_in_look, StandInLook::Magenta);
    assert_eq!(settings.supplied.len(), 1);
    let plain = GpuLaunch::default().settings();
    assert_eq!(plain.stand_in_look, StandInLook::Declared);
    assert!(plain.supplied.is_empty());
}

#[test]
fn the_launch_gives_the_offscreen_settings_the_projects_colour_space() {
    let launch = GpuLaunch {
        linear_colour_space: true,
        ..GpuLaunch::default()
    };
    assert!(launch.settings().linear_colour_space);
    assert!(!GpuLaunch::default().settings().linear_colour_space);
}

#[test]
fn the_report_says_which_globals_the_run_supplied_and_says_nothing_when_it_supplied_none() {
    let directory = tempfile::tempdir().unwrap();
    let none = PlayerGpu::headless(directory.path(), &GpuLaunch::default()).unwrap();
    assert_eq!(none.report().supplied_globals, None);
    let value = SuppliedGlobal::parse("unity_LightData=1,1,1,1").unwrap();
    let some = PlayerGpu::headless(directory.path(), &launch_with(value)).unwrap();
    assert_eq!(
        some.report().supplied_globals.as_deref(),
        Some("globals supplied for this run: unity_LightData=(1, 1, 1, 1)")
    );
}

#[test]
fn the_window_report_says_which_globals_the_run_supplied() {
    let directory = tempfile::tempdir().unwrap();
    let (mut shared, options) = headless::shared([8, 4]);
    let mut source = SharedDevice {
        gpu: &mut shared,
        options: &options,
    };
    let value = SuppliedGlobal::parse("unity_LightData=1,1,1,1").unwrap();
    let gpu = PlayerGpu::shared(&mut source, directory.path(), &launch_with(value)).unwrap();
    assert_eq!(
        gpu.report().supplied_globals.as_deref(),
        Some("globals supplied for this run: unity_LightData=(1, 1, 1, 1)")
    );
}

#[test]
fn the_headless_device_takes_the_budget_the_chooser_gives_its_adapter() {
    let directory = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(directory.path(), &GpuLaunch::default()).unwrap();
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
fn a_frame_that_clears_the_screen_numbers_it_and_frames_after_it_leave_the_number_alone() {
    let _turn = fixture_turn();
    let data = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(data.path(), &GpuLaunch::default()).unwrap();
    gpu.set_screen(Some([8, 4]));
    assert_eq!(gpu.drawn_screen(), None, "nothing drew yet");

    play_screen_fixture(&gpu, 4);

    assert_eq!(
        gpu.drawn_screen(),
        Some(0),
        "both clears were in the first frame's work"
    );
    let image = gpu.offscreen.borrow_mut().read_screen(0).unwrap();
    assert_eq!((image.width, image.height), (8, 4));
    assert!(
        image
            .rgba
            .chunks_exact(4)
            .all(|texel| texel == [255, 255, 0, 255]),
        "the screen is yellow all over"
    );
    let report = gpu.report();
    assert_eq!(report.frames, 4);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn without_a_screen_the_clear_is_left_out_and_no_frame_drew_it() {
    let _turn = fixture_turn();
    let data = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(data.path(), &GpuLaunch::default()).unwrap();

    play_screen_fixture(&gpu, 2);

    assert_eq!(gpu.drawn_screen(), None);
    let report = gpu.report();
    assert!(
        report
            .skipped_by_kind
            .contains_key("SetRenderTarget: UnboundTarget"),
        "{:?}",
        report.skipped_by_kind
    );
}

#[test]
fn a_screen_taken_away_after_a_flush_wrote_it_gives_the_next_frame_no_number() {
    let _turn = fixture_turn();
    let data = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(data.path(), &GpuLaunch::default()).unwrap();
    gpu.set_screen(Some([8, 4]));
    let mut host = booted_fixture_host();
    gpu.install(&mut host).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture/screen.fixture"))
        .unwrap();
    // A flush in the middle of an update draws the screen before the frame is built.
    let snapshot = host.graphics_snapshot().unwrap();
    gpu.offscreen.borrow_mut().flush(&snapshot, &host).unwrap();
    assert!(gpu.offscreen.borrow().screen_written());

    gpu.set_screen(None);
    gpu.frame(&mut host).unwrap();

    assert_eq!(gpu.drawn_screen(), None);
}

#[test]
fn a_player_on_a_callers_device_submits_through_the_callers_authority() {
    let shared =
        worldrunner_render_gpu::test_support::TestGpu::shared(&DeviceRequirements::default())
            .unwrap();
    let authority = SubmissionAuthority::new(shared.queue.clone());
    let directory = tempfile::tempdir().unwrap();
    let budget = AllocationBudget(storage_budget::DEFAULT_BYTES);
    let gpu = PlayerGpu::on(
        shared.device.clone(),
        authority.clone(),
        budget,
        directory.path(),
    )
    .unwrap();
    assert!(gpu.authority.same_authority(&authority));
    assert!(
        gpu.offscreen
            .borrow()
            .authority()
            .same_authority(&authority),
        "the program runtime records and submits through the caller's authority"
    );
    assert!(
        !gpu.offscreen
            .borrow()
            .authority()
            .same_authority(&SubmissionAuthority::new(shared.queue.clone())),
        "a second authority on the same queue is a different one"
    );
    assert_eq!(gpu.storage_budget(), budget.0);
    assert!(gpu.report().device.contains("runtime budget"));
}

/// A viewer with no window and a player on the device it made, with nothing to play yet. The
/// authority is the one the viewer's frames and the player both submit through.
fn window_player(size: [u32; 2]) -> (SharedGpu, SubmissionAuthority, PlayerGpu, tempfile::TempDir) {
    let data = tempfile::tempdir().unwrap();
    let (mut shared, options) = headless::shared(size);
    let gpu = PlayerGpu::shared(
        &mut SharedDevice {
            gpu: &mut shared,
            options: &options,
        },
        data.path(),
        &GpuLaunch::default(),
    )
    .unwrap();
    gpu.set_screen(Some(size));
    let authority = headless::authority(&mut shared, &options);
    (shared, authority, gpu, data)
}

/// Boots the screen fixture on `gpu`, as `play_screen_fixture` does, and advances its first update.
fn started_screen_fixture(gpu: &PlayerGpu) -> DisunityHost {
    let mut host = booted_fixture_host();
    gpu.install(&mut host).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture/screen.fixture"))
        .unwrap();
    host.advance(16_000_000, 1_000_000_000).unwrap();
    host
}

/// What the viewer does with the operations of a frame: admits them on its own recording and
/// submits that, without waiting.
fn submit_like_the_viewer(gpu: &PlayerGpu, operations: PendingOffscreen) {
    let mut frame = gpu.authority.begin(&gpu.device).unwrap();
    let admitted = operations.admit(&frame).unwrap();
    admitted.encode(None, &mut frame).unwrap();
    frame.finish().submit().unwrap();
}

#[test]
fn the_window_and_the_headless_player_ask_for_the_same_device() {
    let data = tempfile::tempdir().unwrap();
    let (mut gpu, options) = headless::shared([8, 4]);
    let mut viewer = SharedDevice {
        gpu: &mut gpu,
        options: &options,
    };
    let adapter = viewer.adapter().unwrap();
    let inputs = PlayerStartupInputs::from_snapshot(Some(6 << 30));
    let settings = Settings::cameras;
    let shared =
        PlayerGpu::shared_on_adapter(&mut viewer, &adapter, data.path(), &inputs, settings())
            .unwrap();
    let headless =
        PlayerGpu::headless_on_adapter(&adapter, data.path(), &inputs, settings()).unwrap();
    let (window, alone) = (shared.device.limits(), headless.device.limits());
    // The viewer asks for as many storage images in a stage as the adapter offers, up to 16, so
    // its device has at least what the player alone asks for.
    assert!(
        window.max_storage_textures_per_shader_stage >= alone.max_storage_textures_per_shader_stage
    );
    assert_eq!(window.max_buffer_size, alone.max_buffer_size);
    assert!(
        shared
            .device
            .features()
            .contains(headless.device.features()),
        "block compression and filterable floats are on the shared device where offered"
    );
    assert_eq!(shared.storage_budget(), headless.storage_budget());
}

/// A source that has no adapter, and counts what it is asked.
#[derive(Default)]
struct NoAdapter {
    asked: usize,
}

impl DeviceSource for NoAdapter {
    fn adapter(&mut self) -> Result<wgpu::Adapter, String> {
        self.asked += 1;
        Err("no adapter".into())
    }

    fn device(
        &mut self,
        _: &DeviceRequirements,
    ) -> Result<(wgpu::Device, SubmissionAuthority), String> {
        self.asked += 1;
        Err("no device".into())
    }
}

#[test]
fn a_window_whose_source_has_no_adapter_fails_naming_it() {
    let data = tempfile::tempdir().unwrap();
    let mut source = NoAdapter::default();
    let error = window_gpu(data.path(), &GpuLaunch::default(), &mut source)
        .err()
        .unwrap();
    assert!(error.to_string().contains("no adapter"), "{error}");
    assert!(source.asked > 0);
}

#[test]
fn a_frame_for_the_viewer_gives_its_operations_and_submits_nothing() {
    let _turn = fixture_turn();
    let (_shared, authority, gpu, _data) = window_player([8, 4]);
    let mut host = started_screen_fixture(&gpu);

    let before = authority.submitted_serial();
    let operations = gpu.frame_for_viewer(&mut host, None).unwrap();

    assert!(
        operations.is_some(),
        "the script's clears are the frame's work"
    );
    assert_eq!(
        authority.submitted_serial(),
        before,
        "the viewer's frame submits it, not the player"
    );
    assert_eq!(
        gpu.drawn_screen(),
        Some(0),
        "the clears handed over are work that draws into the screen"
    );
    let report = gpu.report();
    assert_eq!((report.frames, report.frames_with_work), (1, 1));
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn a_frame_with_no_work_after_the_clears_gives_the_viewer_nothing() {
    let _turn = fixture_turn();
    let (_shared, _authority, gpu, _data) = window_player([8, 4]);
    let mut host = started_screen_fixture(&gpu);
    let operations = gpu.frame_for_viewer(&mut host, None).unwrap().unwrap();
    submit_like_the_viewer(&gpu, operations);

    host.advance(16_000_000, 1_000_000_000).unwrap();
    let next = gpu.frame_for_viewer(&mut host, None).unwrap();

    assert!(next.is_none(), "the script draws nothing more");
    let report = gpu.report();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn the_window_shows_the_screen_texture_itself() {
    let _turn = fixture_turn();
    let (_shared, _authority, gpu, _data) = window_player([8, 4]);
    let mut host = started_screen_fixture(&gpu);
    let operations = gpu.frame_for_viewer(&mut host, None).unwrap().unwrap();
    submit_like_the_viewer(&gpu, operations);
    host.advance(16_000_000, 1_000_000_000).unwrap();
    gpu.frame_for_viewer(&mut host, None).unwrap();

    let screen = gpu.game_screen().expect("the screen texture is offered");

    assert_eq!(
        (screen.texture().width(), screen.texture().height()),
        (8, 4)
    );
    assert!(
        !screen.submitted(),
        "the viewer's frame wrote it, not a flush"
    );
    let report = gpu.report();
    assert_eq!(report.frames_with_work, 1);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
}

#[test]
fn there_is_no_screen_to_offer_before_work_made_it_or_after_it_was_taken_away() {
    let _turn = fixture_turn();
    let (_shared, _authority, gpu, _data) = window_player([8, 4]);
    assert!(gpu.game_screen().is_none(), "no work has made it yet");
    let mut host = started_screen_fixture(&gpu);
    gpu.frame_for_viewer(&mut host, None).unwrap();
    assert!(gpu.game_screen().is_some());

    gpu.set_screen(None);

    assert!(gpu.game_screen().is_none());
}

#[test]
fn a_screen_of_another_size_than_the_window_is_not_offered() {
    let _turn = fixture_turn();
    let (_shared, _authority, gpu, _data) = window_player([8, 4]);
    let mut host = started_screen_fixture(&gpu);
    gpu.frame_for_viewer(&mut host, None).unwrap();
    assert!(gpu.game_screen().is_some());

    gpu.set_screen(Some([16, 8]));

    assert!(
        gpu.game_screen().is_none(),
        "the old texture is the size the window had; the next frame with work makes a new one"
    );
}

/// A plan of two draws through one material the host does not have, drawn by one run: the run
/// has no program to draw it, which the atlas does not need to exist.
fn plan_with_a_run() -> worldrunner_world_simulation::ui::presentation::UiPresentation {
    use worldrunner_world_simulation::graphics::MaterialId;
    use worldrunner_world_simulation::ui::presentation::{
        UiDraw, UiDrawKind, UiMaterial, UiPresentation,
    };
    let draw = UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        corner_colors: None,
        distance_field: None,
        triangle_uv: None,
        corners: [[1., 1., 0.], [5., 1., 0.], [5., 3., 0.], [1., 3., 0.]],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        texture: None,
        material: Some(UiMaterial {
            id: MaterialId::from_bits(0x1_0000_0063),
            generation: 1,
        }),
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0, 0, 8, 4],
    };
    UiPresentation {
        draws: vec![draw],
        ..UiPresentation::default()
    }
}

#[test]
fn the_layer_texture_is_offered_once_a_frame_drew_the_ui_runs_at_the_size_of_the_screen() {
    let _turn = fixture_turn();
    let (_shared, _authority, gpu, _data) = window_player([8, 4]);
    let mut host = started_screen_fixture(&gpu);
    let mut ui = crate::player_ui::PlayerUi::default();
    ui.update(
        plan_with_a_run(),
        crate::player_ui::Screen {
            size: [8, 4],
            max_texture: 4096,
        },
        &worldrunner_render_plan::simulation::SpriteTextures::default(),
        &crate::player_textures::PlayerTextures::default(),
        |_| {},
    )
    .unwrap();
    assert!(gpu.ui_layers().is_none(), "no frame has drawn the runs yet");

    let operations = gpu
        .frame_for_viewer(&mut host, ui.frame())
        .unwrap()
        .expect("the runs are work");
    submit_like_the_viewer(&gpu, operations);

    let layers = gpu.ui_layers().expect("the layer texture is offered");
    assert_eq!([layers.width(), layers.height()], [8, 4]);
    gpu.frame_for_viewer(&mut host, None).unwrap();
    assert!(
        gpu.ui_layers().is_none(),
        "a frame with no runs offers no layer texture"
    );
}

#[test]
fn the_layer_texture_is_the_size_of_the_atlas_the_runs_grew() {
    let _turn = fixture_turn();
    let (_shared, _authority, gpu, _data) = window_player([8, 4]);
    let mut host = started_screen_fixture(&gpu);
    let mut ui = crate::player_ui::PlayerUi::default();
    let mut plan = plan_with_a_run();
    let run = plan.draws[0].clone();
    let whole_screen = [[0., 0., 0.], [8., 0., 0.], [8., 4., 0.], [0., 4., 0.]];
    plan.draws.extend([
        worldrunner_world_simulation::ui::presentation::UiDraw {
            corners: whole_screen,
            clip_rect: [0, 0, 7, 4],
            ..run.clone()
        },
        worldrunner_world_simulation::ui::presentation::UiDraw {
            corners: whole_screen,
            clip_rect: [0, 0, 6, 4],
            ..run
        },
    ]);
    ui.update(
        plan,
        crate::player_ui::Screen {
            size: [8, 4],
            max_texture: 4096,
        },
        &worldrunner_render_plan::simulation::SpriteTextures::default(),
        &crate::player_textures::PlayerTextures::default(),
        |_| {},
    )
    .unwrap();
    let planned = ui.frame().expect("three runs").atlas_size();
    assert_ne!(
        planned,
        [8, 4],
        "three runs of different clips need more than one page"
    );

    let operations = gpu
        .frame_for_viewer(&mut host, ui.frame())
        .unwrap()
        .expect("the runs are work");
    submit_like_the_viewer(&gpu, operations);

    let layers = gpu.ui_layers().expect("the layer texture is offered");
    assert_eq!([layers.width(), layers.height()], planned);
}

#[test]
fn a_flush_the_player_submitted_marks_the_offered_screen_as_written() {
    let _turn = fixture_turn();
    let data = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(data.path(), &GpuLaunch::default()).unwrap();
    gpu.set_screen(Some([8, 4]));
    let mut host = booted_fixture_host();
    gpu.install(&mut host).unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture/screen.fixture"))
        .unwrap();
    let snapshot = host.graphics_snapshot().unwrap();
    gpu.offscreen.borrow_mut().flush(&snapshot, &host).unwrap();

    let screen = gpu.game_screen().expect("the flush made the screen");

    assert!(
        screen.submitted(),
        "the flush ran in a submission of the player's own, before any frame the viewer carries"
    );

    gpu.set_screen(Some([16, 8]));
    let (writer, _) = gpu
        .offscreen
        .borrow_mut()
        .frame(&snapshot, &host, Settling::Wait, &[])
        .unwrap();
    assert!(writer.is_some());
    let resized = gpu.game_screen().expect("the frame allocated a new screen");
    assert_eq!(
        (resized.texture().width(), resized.texture().height()),
        (16, 8)
    );
    assert!(!resized.submitted(), "the flush wrote the old allocation");
}

fn reads(set: &[&str], read: &[&str]) -> Report {
    Report {
        global_reads: GlobalReads {
            set: names(set),
            read: names(read),
        },
        ..Report::default()
    }
}

#[test]
fn the_run_report_lists_a_global_set_in_one_piece_of_work_and_read_in_another_as_read() {
    let mut report = GpuWorkReport::default();
    report.add(&reads(&["Minimap", "_Grab", "Other"], &[]), "frame");
    report.add(&reads(&[], &["_Grab", "Minimap"]), "flush");
    assert_eq!(
        report.global_reads.line(TOP_UNREAD_GLOBALS).as_deref(),
        Some("globals set and never read by a draw: Other")
    );
}

#[test]
fn the_unread_globals_line_names_the_first_thirty_and_counts_the_rest() {
    let all: Vec<String> = (0..32).map(|n| format!("G{n:02}")).collect();
    let set: Vec<&str> = all.iter().map(String::as_str).collect();
    let mut report = GpuWorkReport::default();
    report.add(&reads(&set, &[]), "frame");
    let line = report.global_reads.line(TOP_UNREAD_GLOBALS).unwrap();
    assert!(line.ends_with("G29, and 2 more"), "{line}");
    assert!(!line.contains("G30"), "{line}");
    assert_eq!(
        GpuWorkReport::default()
            .global_reads
            .line(TOP_UNREAD_GLOBALS),
        None
    );
}
