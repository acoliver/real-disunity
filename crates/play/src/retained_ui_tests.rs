use super::*;
use std::{path::PathBuf, process::Command, time::Instant};
use worldrunner_disunity_runtime::ManagedHostConfig;

/// The turn of one test that boots the managed host. A process holds one host at a time, so a
/// second test booting while the first runs fails with `AlreadyBooted`, and two fixture builds
/// at once can collide on the build's output files. Hold the turn for the whole test.
pub(crate) fn fixture_turn() -> std::sync::MutexGuard<'static, ()> {
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
    TURN.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A host booted on the portable fixture's managed assembly, with no slice loaded. Take
/// [`fixture_turn`] first.
pub(crate) fn booted_fixture_host() -> DisunityHost {
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
    DisunityHost::boot(&ManagedHostConfig {
        ffi_library: target.join("debug").join(format!(
            "{}worldrunner_disunity_ffi{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX,
        )),
        runtime_config: managed.join("PortableFixture.runtimeconfig.json"),
        assembly: managed.join("PortableFixture.dll"),
        bridge_type: "PortableFixture.NativeBridge, PortableFixture".into(),
    })
    .unwrap()
}

fn fixture_host() -> DisunityHost {
    let mut host = booted_fixture_host();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    host.run_slice(&root.join("csharp/PortableFixture/retained-ui.fixture"))
        .unwrap();
    host
}

#[test]
fn retained_player_draws_text_images_and_clipped_children_without_redundant_ui_updates() {
    let _turn = fixture_turn();
    let mut driver = PlayerRetainedDriver::new(
        fixture_host(),
        PlayerWindow {
            title: "windowless player UI".into(),
            size: [64, 64],
            fixed_dt: Some(Duration::from_millis(16)),
            max_frames: None,
            capture_frames: BTreeSet::from([0, 1]),
            keys: vec![],
        },
        crate::scene_light::SceneLighting::start(&[], viewer_options_camera())
            .unwrap()
            .0,
    )
    .unwrap();
    let scene = SceneTables::initial_scene(None).unwrap();
    let host = RetainedHost::new(scene, viewer_options(&driver.window)).unwrap();
    let mut run = host.windowless(&mut driver).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    run.frame().unwrap();
    let generation = run.host().accepted_generation();
    for _ in 0..7 {
        run.frame().unwrap();
        run.poll().unwrap();
        assert_eq!(run.host().accepted_generation(), generation);
    }
    while run.poll().unwrap() {
        assert!(Instant::now() < deadline, "capture timed out");
        std::thread::yield_now();
    }
    run.shutdown().unwrap();
    drop(run);
    assert_eq!(driver.ui.updates, 1, "unchanged frames send no UI update");
    assert_eq!(driver.captures.len(), 2);
    for image in &driver.captures {
        assert_ui_pixels(image);
    }
    assert_eq!(driver.captures[0].pixels, driver.captures[1].pixels);
}

fn viewer_options_camera() -> SceneCamera {
    viewer_options(&PlayerWindow {
        title: String::new(),
        size: [1, 1],
        fixed_dt: None,
        max_frames: None,
        capture_frames: BTreeSet::new(),
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

fn rgba(image: &Capture, x: u32, y: u32) -> &[u8] {
    let offset = ((image.height - 1 - y) * image.width + x) as usize * 4;
    &image.pixels[offset..offset + 4]
}
