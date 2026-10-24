use worldrunner_render_scene::{CameraPose, OrthographicLens, SceneView, Viewport};
use worldrunner_scene_viewer::{
    LiveDriver, LiveFrame, LiveInputEvent, LiveInputSample, LiveOptions, LiveReceipt, LiveTick,
    ScheduledInput, ViewerError, run_live,
};

struct Driver<'a> {
    case: &'a str,
    initialized: usize,
    ticks: Vec<LiveTick>,
    receipts: usize,
}

impl LiveDriver for Driver<'_> {
    fn initialize(&mut self, _: Viewport) -> Result<(), String> {
        self.initialized += 1;
        if self.case == "initialize-error" {
            return Err("initialize sentinel".into());
        }
        Ok(())
    }

    fn ingress(&mut self, _: &LiveInputSample) -> Result<(), String> {
        if self.case == "ingress-error" {
            return Err("ingress sentinel".into());
        }
        Ok(())
    }

    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        self.ticks.push(tick.clone());
        if self.case == "update-error" {
            return Err("update sentinel".into());
        }
        let mut viewport = tick.viewport;
        if self.case == "invalid-view" {
            viewport.width += 1;
        }
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
            viewport,
        )
        .unwrap();
        Ok(LiveFrame {
            view,
            quads: vec![],
            textures: None,
            clear: if self.case == "invalid-clear" {
                [f64::NAN; 4]
            } else {
                [1.0, 0.0, 0.0, 1.0]
            },
            capture: (tick.frame == 2 || self.case == "frame-exit").then_some(17),
            exit: self.case == "frame-exit",
        })
    }

    fn presented(&mut self, receipt: LiveReceipt) -> Result<(), String> {
        assert_eq!(receipt.frame, u64::try_from(self.receipts).unwrap());
        self.receipts += 1;
        if self.case == "receipt-error" {
            return Err("receipt sentinel".into());
        }
        if receipt.frame == 2 || self.case == "frame-exit" {
            let capture = receipt.capture.unwrap();
            assert_eq!(capture.id, 17);
            assert!(
                capture
                    .image
                    .pixels
                    .chunks_exact(4)
                    .all(|pixel| pixel == [255, 0, 0, 255])
            );
        } else {
            assert!(receipt.capture.is_none());
        }
        Ok(())
    }
}

pub(super) fn run(case: &str) {
    let mut options = LiveOptions::new(format!("live lifecycle {case}"), [64, 64]);
    options.max_frames = Some(4);
    if case == "synthetic-close" {
        options.scheduled_input.push(ScheduledInput {
            frame: 2,
            event: LiveInputEvent::CloseRequested,
        });
    }
    configure_ingress(case, &mut options);
    let mut driver = Driver {
        case,
        initialized: 0,
        ticks: vec![],
        receipts: 0,
    };
    let result = run_live(&mut driver, options);
    assert_eq!(driver.initialized, 1);
    match case {
        "ingress-error" => {
            assert_driver_error(result, "ingress sentinel");
            assert!(driver.ticks.is_empty());
            assert_eq!(driver.receipts, 0);
        }
        "channel-budget" | "scheduled-budget" => {
            assert_driver_error(
                result,
                "live ingress exceeds 4096 observations before update",
            );
            assert!(driver.ticks.is_empty());
            assert_eq!(driver.receipts, 0);
        }
        "initialize-error" => {
            assert_driver_error(result, "initialize sentinel");
            assert!(driver.ticks.is_empty());
        }
        "update-error" => {
            assert_driver_error(result, "update sentinel");
            assert_eq!(driver.receipts, 0);
        }
        "receipt-error" => {
            assert_driver_error(result, "receipt sentinel");
            assert_eq!(driver.receipts, 1);
        }
        "invalid-clear" | "invalid-view" => {
            assert!(matches!(result, Err(ViewerError::InvalidOptions(_))));
            assert_eq!(driver.receipts, 0);
        }
        _ => {
            result.unwrap();
            verify_completion(&driver);
        }
    }
    println!("live lifecycle {case} passed");
}

fn assert_driver_error(result: Result<(), ViewerError>, expected: &str) {
    assert!(matches!(result, Err(ViewerError::Driver(message)) if message == expected));
}

fn verify_completion(driver: &Driver<'_>) {
    let count = match driver.case {
        "frame-exit" => 1,
        "synthetic-close" => 3,
        _ => 4,
    };
    assert_eq!(driver.ticks.len(), count);
    assert_eq!(driver.receipts, count);
    assert!(driver.ticks[0].dt.is_zero());
    assert!(driver.ticks[0].elapsed.is_zero());
    for pair in driver.ticks.windows(2) {
        assert_eq!(pair[1].elapsed, pair[0].elapsed + pair[0].dt);
        assert!(!pair[1].dt.is_zero());
    }
    if driver.case == "synthetic-close" {
        assert!(
            driver
                .ticks
                .last()
                .unwrap()
                .input
                .contains(&LiveInputEvent::CloseRequested)
        );
    }
}

fn configure_ingress(case: &str, options: &mut LiveOptions) {
    if case == "ingress-error" {
        options.scheduled_input.push(ScheduledInput {
            frame: 0,
            event: LiveInputEvent::CaptureRequested(1),
        });
    }
    if case == "scheduled-budget" {
        options.scheduled_input = (0..4097)
            .map(|id| ScheduledInput {
                frame: 0,
                event: LiveInputEvent::CaptureRequested(id),
            })
            .collect();
    }
    if case == "channel-budget" {
        let (sender, receiver) = std::sync::mpsc::channel();
        for id in 0..4097 {
            sender.send(LiveInputEvent::CaptureRequested(id)).unwrap();
        }
        options.input = Some(receiver);
    }
}
