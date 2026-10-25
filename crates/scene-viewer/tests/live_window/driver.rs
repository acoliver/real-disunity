use std::{collections::BTreeSet, sync::mpsc::Sender, time::Duration};

use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};
use worldrunner_render_scene::{
    CameraPose, OrthographicLens, SceneView, Viewport, dynamic::DynamicQuad, offscreen::RgbaImage,
};
use worldrunner_scene_viewer::{LiveDriver, LiveFrame, LiveInputEvent, LiveReceipt, LiveTick};

use super::os_window;

#[derive(Default)]
pub(super) struct Driver {
    pub initialized: usize,
    pub ticks: Vec<LiveTick>,
    pub receipts: Vec<u64>,
    pub images: Vec<RgbaImage>,
    pub held: BTreeSet<String>,
    pub sender: Option<Sender<LiveInputEvent>>,
    pub os_pressed: bool,
    pub os_released: bool,
    x: f32,
}

impl Driver {
    pub fn new(sender: Sender<LiveInputEvent>) -> Self {
        Self {
            sender: Some(sender),
            ..Self::default()
        }
    }

    fn input(&mut self, input: &LiveInputEvent) {
        if let LiveInputEvent::Key { key, pressed, .. } = input {
            if key == "arrowright" {
                if *pressed {
                    self.os_pressed = true;
                } else {
                    self.os_released = true;
                }
            }
            if *pressed {
                self.held.insert(key.clone());
            } else {
                self.held.remove(key);
            }
        }
    }
}

impl LiveDriver for Driver {
    fn initialize(&mut self, viewport: Viewport) -> Result<(), String> {
        assert_eq!(viewport.width, 2 * viewport.height);
        assert!(viewport.height >= 64);
        self.initialized += 1;
        Ok(())
    }

    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        assert_eq!(tick.frame, u64::try_from(self.ticks.len()).unwrap());
        assert_eq!(tick.elapsed, Duration::from_millis(20 * tick.frame));
        assert_eq!(tick.dt, Duration::from_millis(20));
        println!("tick {} input {:?}", tick.frame, tick.input);
        for input in &tick.input {
            self.input(input);
        }
        if self.held.contains("a") {
            self.x += 0.5;
        }
        self.ticks.push(tick.clone());
        let view = view(tick.viewport);
        Ok(LiveFrame {
            view,
            quads: vec![DynamicQuad {
                position: [self.x, 0.0, 0.0],
                texture: Some(0),
                tint: [1.0, 0.0, 0.0, 1.0],
                ..DynamicQuad::default()
            }],
            textures: (tick.frame == 0).then(|| vec![white_texture()]),
            clear: [0.0, 0.0, 0.0, 1.0],
            capture: Some(capture_id(tick)),
            exit: false,
        })
    }

    fn presented(&mut self, receipt: LiveReceipt) -> Result<(), String> {
        assert_eq!(receipt.frame, u64::try_from(self.receipts.len()).unwrap());
        let tick = self.ticks.last().unwrap();
        assert_eq!(receipt.elapsed, tick.elapsed);
        let capture = receipt.capture.expect("requested actual presented pixels");
        assert_eq!(capture.id, capture_id(tick));
        assert_eq!(capture.image.viewport, tick.viewport);
        os_window::landmarks(&capture.image, receipt.frame);
        self.images.push(capture.image);
        match receipt.frame {
            0 => os_window::key("keydown"),
            2 => os_window::key("keyup"),
            3 => os_window::resize(),
            4 => self
                .sender
                .as_ref()
                .unwrap()
                .send(LiveInputEvent::CaptureRequested(909))
                .unwrap(),
            _ => {}
        }
        self.receipts.push(receipt.frame);
        Ok(())
    }
}

fn white_texture() -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255; 4].into(),
    }
}

pub(super) fn view(viewport: Viewport) -> SceneView {
    SceneView::orthographic(
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
    .unwrap()
}

fn capture_id(tick: &LiveTick) -> u64 {
    tick.input
        .iter()
        .find_map(|event| match event {
            LiveInputEvent::CaptureRequested(id) => Some(*id),
            _ => None,
        })
        .unwrap_or(100 + tick.frame)
}
