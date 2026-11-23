use super::*;
use crate::GpuLaunch;
use crate::gpu_device::{SharedDevice, headless};
use std::{path::PathBuf, process::Command, sync::OnceLock, time::Instant};
use worldrunner_disunity_runtime::ManagedHostConfig;

/// The turn of one test that boots the managed host. A process holds one host at a time, so a
/// second test booting while the first runs fails with `AlreadyBooted`. Hold the turn for the
/// whole test.
pub(crate) fn fixture_turn() -> std::sync::MutexGuard<'static, ()> {
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    TURN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The configuration that boots the portable fixture's managed assembly. The first call in a test
/// process builds the FFI library and the fixture; later calls return the same files and build
/// nothing. The CLR lives as long as the process and keeps calling the FFI image it loaded first,
/// while a cargo build replaces the library file even when nothing compiles (on macOS a new
/// inode), so a host booted after a second build would load a second image whose store holds none
/// of the worlds the managed side made. Take [`fixture_turn`] first.
pub(crate) fn fixture_config() -> ManagedHostConfig {
    static CONFIG: OnceLock<ManagedHostConfig> = OnceLock::new();
    CONFIG.get_or_init(build_fixture).clone()
}

fn build_fixture() -> ManagedHostConfig {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = Command::new(env!("CARGO"))
        .args(["build", "--locked", "-p", "worldrunner-disunity-ffi"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let project = root.join("csharp/PortableFixture/PortableFixture.csproj");
    let status = Command::new("dotnet")
        .args(["build", "-warnaserror"])
        .arg(project)
        .status()
        .unwrap();
    assert!(status.success());
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let managed = root.join("csharp/PortableFixture/bin/Debug/net8.0");
    ManagedHostConfig {
        ffi_library: target.join("debug").join(format!(
            "{}worldrunner_disunity_ffi{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX,
        )),
        runtime_config: managed.join("PortableFixture.runtimeconfig.json"),
        assembly: managed.join("PortableFixture.dll"),
        bridge_type: "PortableFixture.NativeBridge, PortableFixture".into(),
    }
}

/// A host booted on the portable fixture's managed assembly, with no slice loaded. Take
/// [`fixture_turn`] first.
pub(crate) fn booted_fixture_host() -> DisunityHost {
    DisunityHost::boot(&fixture_config()).unwrap()
}

fn fixture_host(fixture: &str) -> DisunityHost {
    let mut host = booted_fixture_host();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture").join(fixture))
        .unwrap();
    host
}

#[test]
fn every_host_booted_in_one_test_process_reaches_the_world_its_slice_loaded() {
    let _turn = fixture_turn();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for boot in 0..2 {
        let mut host = booted_fixture_host();
        host.run_slice(&root.join("csharp/PortableFixture/triangle.fixture"))
            .unwrap();
        let objects = host.export_objects().map(|states| states.len());
        assert!(matches!(objects, Ok(3)), "boot {boot}: {objects:?}");
    }
}

/// The frames a fixture's player captured and the UI updates it sent.
pub(crate) struct FixtureRun {
    pub(crate) captures: Vec<Capture>,
    pub(crate) ui_updates: u64,
}

/// Plays a portable fixture (its `.fixture` file name) in a 64 by 64 windowless player for eight
/// frames and returns what frames 0 and 1 captured. Take [`fixture_turn`] first.
pub(crate) fn play_fixture(fixture: &str) -> FixtureRun {
    let data = tempfile::tempdir().unwrap();
    let (mut shared, options) = headless::shared([64, 64]);
    let gpu = crate::player::window_gpu(
        data.path(),
        &GpuLaunch::default(),
        &mut SharedDevice {
            gpu: &mut shared,
            options: &options,
        },
    )
    .expect("the test needs a GPU adapter");
    let window = PlayerWindow {
        title: "windowless player UI".into(),
        size: [64, 64],
        fixed_dt: Some(Duration::from_millis(16)),
        max_frames: Some(2),
        capture_frames: BTreeSet::from([0, 1]),
        capture_files: Vec::new(),
        keys: vec![],
    };
    let mut driver = PlayerRetainedDriver::new(
        fixture_host(fixture),
        &window,
        crate::scene_light::SceneLighting::start(&[], viewer_options_camera())
            .unwrap()
            .0,
        gpu,
        false,
    )
    .unwrap();
    let scene = SceneTables::initial_scene(None).unwrap();
    let host = RetainedHost::new(scene, options).unwrap();
    let mut run = host.windowless(&mut driver, Some(shared)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    run.frame().unwrap();
    let generation = run.host().accepted_generation();
    while !run.is_closed() {
        assert!(Instant::now() < deadline, "the run timed out");
        run.frame().unwrap();
        if !run.is_closed() {
            run.poll().unwrap();
        }
        assert_eq!(run.host().accepted_generation(), generation);
        std::thread::yield_now();
    }
    run.shutdown().unwrap();
    drop(run);
    FixtureRun {
        ui_updates: driver.ui.updates,
        captures: driver.script.finish().unwrap(),
    }
}

#[test]
fn retained_player_draws_text_images_and_clipped_children_without_redundant_ui_updates() {
    let _turn = fixture_turn();
    let run = play_fixture("retained-ui.fixture");
    assert_eq!(run.ui_updates, 1, "unchanged frames send no UI update");
    assert_eq!(run.captures.len(), 2);
    for image in &run.captures {
        assert_ui_pixels(image);
    }
    assert_eq!(run.captures[0].pixels, run.captures[1].pixels);
}

fn viewer_options_camera() -> SceneCamera {
    viewer_options(&PlayerWindow {
        title: String::new(),
        size: [1, 1],
        fixed_dt: None,
        max_frames: None,
        capture_frames: BTreeSet::new(),
        capture_files: Vec::new(),
        keys: vec![],
    })
    .camera
}

fn assert_ui_pixels(image: &Capture) {
    for y in 24..32 {
        for x in 24..32 {
            assert_eq!(rgba(image, x, y), [0, 255, 0, 255], "{x},{y}");
        }
    }
    for y in 8..16 {
        for x in 8..16 {
            assert_eq!(rgba(image, x, y), [0, 0, 255, 255], "{x},{y}");
        }
    }
    for y in 25..29 {
        for x in 17..21 {
            assert_eq!(rgba(image, x, y), [255; 4], "text pixel {x},{y}");
        }
    }
    let green = image
        .pixels
        .chunks_exact(4)
        .filter(|p| *p == [0, 255, 0, 255])
        .count();
    assert_eq!(green, 64, "the child draws only inside its clip bounds");
    for y in 4..8 {
        for x in 4..60 {
            assert_eq!(rgba(image, x, y), [255, 255, 0, 255], "back canvas {x},{y}");
        }
    }
    let scene = rgba(image, 2, 2);
    assert!(
        scene[0] > 0 && scene[1..] == [0, 0, 255],
        "scene mesh draws behind UI: {scene:?}"
    );
}

/// The pixel of a capture, with y counted from the bottom row of the window.
pub(crate) fn rgba(image: &Capture, x: u32, y: u32) -> &[u8] {
    let offset = ((image.height - 1 - y) * image.width + x) as usize * 4;
    &image.pixels[offset..offset + 4]
}
