use std::{
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
use worldrunner_render_scene::{CameraPose, OrthographicLens, SceneView};
use worldrunner_scene_viewer::{
    IngressOrigin, LiveClock, LiveDriver, LiveFrame, LiveInputEvent, LiveInputSample, LiveOptions,
    LiveReceipt, LiveTick, run_live,
};

#[path = "ingress/os_window.rs"]
mod os_window;

const TITLE: &str = "worldrunner-timestamped-ingress";
const SINK: &str = "worldrunner-ingress-focus-sink";

struct FocusSink(Child);
impl Drop for FocusSink {
    fn drop(&mut self) {
        let _ = self.0.kill();
        self.0.wait().unwrap();
    }
}

#[derive(Default)]
struct Driver {
    pending: Vec<LiveInputSample>,
    batches: Vec<Vec<LiveInputSample>>,
    ticks: Vec<LiveTick>,
    receipts: usize,
    sender: Option<mpsc::Sender<LiveInputEvent>>,
    clock: LiveClock,
}

impl LiveDriver for Driver {
    fn clock(&self) -> LiveClock {
        self.clock
    }

    fn ingress(&mut self, sample: &LiveInputSample) -> Result<(), String> {
        self.pending.push(sample.clone());
        Ok(())
    }

    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        assert_eq!(
            self.pending
                .iter()
                .map(|s| s.event.clone())
                .collect::<Vec<_>>(),
            tick.input
        );
        println!("timestamped frame {}: {:?}", tick.frame, self.pending);
        self.batches.push(std::mem::take(&mut self.pending));
        self.ticks.push(tick.clone());
        let view = SceneView::orthographic(
            CameraPose {
                eye_millimeters: [0, -10_000, 0],
                target_millimeters: [0; 3],
            },
            OrthographicLens {
                vertical_half_extent_meters: 2.0,
                near_meters: 0.1,
                far_meters: 100.0,
            },
            tick.viewport,
        )
        .unwrap();
        Ok(LiveFrame {
            view,
            quads: vec![],
            textures: None,
            clear: [0.0, 0.0, 0.0, 1.0],
            capture: None,
            exit: false,
        })
    }

    fn presented(&mut self, receipt: LiveReceipt) -> Result<(), String> {
        self.receipts += 1;
        match receipt.frame {
            0 => {
                os_window::focus(TITLE);
                os_window::xdo(&["keydown", "a", "keyup", "a"]);
            }
            1 => os_window::xdo(&["keydown", "b"]),
            3 => os_window::xdo(&["keyup", "b"]),
            4 => {
                os_window::xdo(&["keydown", "d"]);
                os_window::focus(SINK);
            }
            5 => os_window::focus(TITLE),
            6 => {
                os_window::xdo(&["keyup", "d"]);
                os_window::xdo(&["key", "F5", "F12", "Escape"]);
            }
            7 => {
                let sender = self.sender.as_ref().unwrap();
                sender.send(LiveInputEvent::CaptureRequested(91)).unwrap();
                sender
                    .send(LiveInputEvent::Key {
                        key: "c".into(),
                        pressed: true,
                        repeat: true,
                    })
                    .unwrap();
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn run() {
    run_clock(LiveClock::FrameDelta);
}

pub(super) fn acquisition() {
    run_clock(LiveClock::Acquisition);
}

fn run_clock(clock: LiveClock) {
    assert!(
        Command::new("setxkbmap")
            .args(["-layout", "us"])
            .status()
            .unwrap()
            .success()
    );
    let mut sink = FocusSink(
        Command::new("xmessage")
            .args(["-name", SINK, "-title", SINK, "focus sink"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    os_window::wait_window(SINK, &mut sink.0);
    let mut options = LiveOptions::new(TITLE, [96, 64]);
    options.frame_interval = Duration::from_millis(150);
    options.max_frames = Some(10);
    let (sender, receiver) = mpsc::channel();
    options.input = Some(receiver);
    let mut driver = Driver {
        clock,
        sender: Some(sender),
        ..Driver::default()
    };
    run_live(&mut driver, options).unwrap();
    assert_eq!(driver.receipts, 10);
    os_window::verify(&driver);
    println!(
        "timestamped ingress: real OS between-frame short tap, held/release, focus loss/regain, reserved controls, channel repeat/provenance and frame-delta input passed"
    );
}
