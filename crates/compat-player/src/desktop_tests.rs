use crate::{desktop::Desktop, playback::Playback};
use compat_kit::{RunRequest, WorkerReport};
use serde_json::json;
use std::{path::Path, time::Duration};
use worldrunner::scene::{
    gpu::Viewport,
    viewer::{LiveDriver, LiveInputEvent, LiveTick},
};

#[test]
fn live_ticks_continue_after_load_and_os_input_advances_the_same_world() {
    let _host = crate::one_host::one_host_at_a_time();
    let installation = crate::test_support::installation();
    let artifacts = crate::test_support::scratch("desktop-");
    let request = request(artifacts.path());
    let mut report = report();
    let mut session = None;
    let playback = Playback::new(&installation, &request, &mut report, &mut session).unwrap();
    let mut driver = Desktop::new(playback).unwrap();
    let first = driver.update(&tick(0, vec![])).unwrap();
    assert!(
        !first.exit,
        "interactive mode must keep presenting after its load action"
    );
    present(&mut driver, first, 0);
    let initial = driver
        .playback
        .active()
        .unwrap()
        .observe("world.snapshot")
        .unwrap();
    for frame in 1..=20 {
        let keys = if frame == 1 {
            vec![key("w", true)]
        } else {
            vec![]
        };
        let output = driver.update(&tick(frame, keys)).unwrap();
        assert!(!output.exit);
        present(&mut driver, output, frame);
    }
    let moved = driver
        .playback
        .active()
        .unwrap()
        .observe("world.snapshot")
        .unwrap();
    assert_ne!(
        initial["objects"], moved["objects"],
        "OS input and live delta must change native objects without authored advances"
    );
    capture_reload_stop(&mut driver);
    driver.playback.active().unwrap().unload().unwrap();
    assert_eq!(driver.playback.active().unwrap().statistics().unwrap().0, 0);
    assert_eq!(
        driver.playback.active().unwrap().managed_roots().unwrap(),
        0
    );
}

pub(super) fn request(artifacts: &Path) -> RunRequest {
    let mut request = crate::test_support::fixture_request("snake", artifacts, true);
    request.profile = "compat-6000.3".into();
    request.metadata["interactive"] =
        json!({"duration_ms":30000,"max_frames":1800,"max_captures":8});
    request.assertions = vec![];
    request.timeline = vec![json!({"frame":0,"phase":"load","action":"load"})];
    request
}

pub(super) fn report() -> WorkerReport {
    serde_json::from_value(
        json!({"schema_version":1,"evidence_label":"original-source/authored-missing-sprite",
        "observations":[],"captures":[],"diagnostics":[],
        "teardown":{"completed":false,"native_worlds":0,"managed_roots":0,"pending_readbacks":0}}),
    )
    .unwrap()
}

pub(super) fn tick(frame: u64, input: Vec<LiveInputEvent>) -> LiveTick {
    LiveTick {
        frame,
        elapsed: Duration::from_millis(frame * 20),
        dt: Duration::from_millis(20),
        viewport: Viewport {
            width: 960,
            height: 720,
        },
        input,
    }
}

fn key(key: &str, pressed: bool) -> LiveInputEvent {
    LiveInputEvent::Key {
        key: key.into(),
        pressed,
        repeat: false,
    }
}

pub(super) fn present(
    driver: &mut Desktop<'_>,
    frame: worldrunner::scene::viewer::LiveFrame,
    index: u64,
) {
    use crate::{
        extraction::{AssetTable, Contribution},
        graphics::Graphics,
    };
    use worldrunner::scene::viewer::{LiveCapture, LiveReceipt};
    let capture = frame.capture.map(|id| {
        let graph = &driver.playback.session.as_ref().unwrap().graph;
        let mut assets = AssetTable::load(&driver.playback.request.source_root, graph).unwrap();
        assets.textures = frame
            .textures
            .unwrap_or_else(|| driver.playback.textures().unwrap());
        let mut graphics = Graphics::new(&assets, tick(index, vec![]).viewport).unwrap();
        let image = graphics
            .render(&Contribution {
                view: frame.view,
                quads: frame.quads,
                clear: frame.clear,
            })
            .unwrap();
        LiveCapture { id, image }
    });
    driver
        .presented(LiveReceipt {
            frame: index,
            elapsed: Duration::from_millis(index * 20),
            capture,
        })
        .unwrap();
}

fn capture_reload_stop(driver: &mut Desktop<'_>) {
    let before = driver
        .playback
        .active()
        .unwrap()
        .observe("world.snapshot")
        .unwrap();
    let mut capture_tick = tick(21, vec![key("f12", true)]);
    capture_tick.dt = Duration::ZERO;
    let captured = driver.update(&capture_tick).unwrap();
    assert_eq!(captured.capture, Some(21));
    let after = driver
        .playback
        .active()
        .unwrap()
        .observe("world.snapshot")
        .unwrap();
    assert_eq!(
        before, after,
        "capture and extraction must not advance gameplay"
    );
    present(driver, captured, 21);
    assert_eq!(driver.playback.report.captures.len(), 2);
    assert_eq!(driver.playback.report.observations.len(), 4);
    assert!(
        driver
            .playback
            .request
            .artifact_root
            .join("captures/interactive-000021.png")
            .is_file()
    );
    reload_and_stop(driver, &before);
}

fn reload_and_stop(driver: &mut Desktop<'_>, before: &serde_json::Value) {
    let reset = driver.update(&tick(22, vec![key("f5", true)])).unwrap();
    present(driver, reset, 22);
    let input = driver
        .playback
        .active()
        .unwrap()
        .request(json!({"op":"input.get"}))
        .unwrap();
    assert_eq!(
        input["keys"],
        json!([]),
        "reload must clear native held keys"
    );
    let reloaded = driver
        .playback
        .active()
        .unwrap()
        .observe("world.snapshot")
        .unwrap();
    assert_ne!(
        before["objects"][0]["handle"],
        reloaded["objects"][0]["handle"]
    );
    let resumed = driver.update(&tick(23, vec![key("d", true)])).unwrap();
    assert!(!resumed.exit);
    present(driver, resumed, 23);
    let input = driver
        .playback
        .active()
        .unwrap()
        .request(json!({"op":"input.get"}))
        .unwrap();
    assert_eq!(input["keys"], json!(["D"]));
    exhaust_capture_budget(driver);
    let stop = driver.update(&tick(41, vec![key("escape", true)])).unwrap();
    assert!(stop.exit);
    present(driver, stop, 41);
}

fn exhaust_capture_budget(driver: &mut Desktop<'_>) {
    for frame in 24..=40 {
        let events = vec![key("f12", false), key("f12", true)];
        let mut capture_tick = tick(frame, events);
        capture_tick.dt = Duration::ZERO;
        let output = driver.update(&capture_tick).unwrap();
        assert!(!output.exit);
        present(driver, output, frame);
    }
    assert_eq!(driver.playback.report.captures.len(), 8);
    assert_eq!(driver.playback.report.observations.len(), 16);
}
