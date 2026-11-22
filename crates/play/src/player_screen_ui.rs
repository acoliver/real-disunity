//! The frame a headless `--screen` run writes: the game's screen with the game's UI over it, as a
//! window shows it.
//!
//! The UI is drawn by the viewer, in the one retained session every window frame comes from. A
//! screen run therefore makes its device through the viewer's [`SharedGpu`], builds the player's
//! offscreen work on it as a window does, and keeps a windowless session on the same device. The
//! run's own frames still go through the player's submit-and-wait path. At each frame the run is
//! asked to write, the session is given the screen texture as written, the frame's UI through
//! [`PlayerUi`] as a window gives it, and takes one capture of the composed frame. Nothing is
//! copied through the CPU between the screen and the capture.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_gpu::{device::DeviceRequirements, submission::SubmissionAuthority};
use worldrunner_render_plan::simulation::SpriteTextures;
use worldrunner_scene_viewer::{
    RenderClock, RetainedCaptureOutcome, RetainedDriver, RetainedEvent, RetainedEventHost,
    RetainedHost, RetainedTick, SharedGpu, ViewerError, ViewerOptions, WindowlessRetained,
};

use crate::{
    PlayError,
    driver::Capture,
    gpu_device::DeviceSource,
    gpu_player::PlayerGpu,
    player::{PreparedPlayer, window_gpu},
    player_textures::PlayerTextures,
    player_ui::{PlayerUi, graphic_textures},
    retained_driver::{PlayerWindow, viewer_options},
    scene_tables::SceneTables,
};

/// How long one composed frame may take. A software adapter draws a large frame in seconds; a
/// capture that has not arrived by now is stuck.
const COMPOSE_TIMEOUT: Duration = Duration::from_mins(5);

/// The instance of the composing session's device: the headless one, and render-gpu's test
/// instance in play's tests, as for the headless player's device (see `headless_gpu_instance`).
#[cfg(not(test))]
fn headless_shared_gpu() -> SharedGpu {
    SharedGpu::headless()
}

#[cfg(test)]
fn headless_shared_gpu() -> SharedGpu {
    SharedGpu::headless_for_test()
}

/// The viewer's options for a session of `size` that renders at a fixed time, so a frame does not
/// depend on the wall clock.
fn session_options(size: [u32; 2]) -> ViewerOptions {
    let window = PlayerWindow {
        title: "player screen".into(),
        size,
        fixed_dt: None,
        max_frames: None,
        capture_frames: BTreeSet::new(),
        keys: Vec::new(),
    };
    ViewerOptions {
        render_clock: RenderClock::Fixed(Duration::ZERO),
        ..viewer_options(&window)
    }
}

fn viewer_failed(what: &str, error: &ViewerError) -> PlayError {
    PlayError::Invalid(format!("{what}: {error}"))
}

/// The device a screen run's offscreen work and its composing session share. The viewer makes it
/// for its own needs and the player's, before the scene loads.
pub(crate) struct ScreenDevice {
    gpu: SharedGpu,
    options: ViewerOptions,
    size: [u32; 2],
}

impl ScreenDevice {
    pub(crate) fn new(size: [u32; 2]) -> Self {
        Self {
            gpu: headless_shared_gpu(),
            options: session_options(size),
            size,
        }
    }

    /// The offscreen work of the run on this device.
    ///
    /// # Errors
    /// Fails naming the reason when the device cannot be had.
    pub(crate) fn player_gpu(&mut self, prepared: &PreparedPlayer) -> Result<PlayerGpu, PlayError> {
        window_gpu(&prepared.data, &prepared.gpu, self)
    }

    /// The windowless session that composes the run's frames on this device, delivering its
    /// captures through `sink`.
    pub(crate) fn compose(self, sink: &mut CaptureSink) -> Result<ScreenComposer<'_>, PlayError> {
        let scene = SceneTables::initial_scene(None)
            .map_err(|error| PlayError::Invalid(format!("the screen cannot be drawn: {error}")))?;
        let host = RetainedHost::new(scene, self.options)
            .map_err(|error| viewer_failed("the screen's viewer cannot start", &error))?;
        let delivered = Rc::clone(&sink.delivered);
        let run = host
            .windowless(sink, Some(self.gpu))
            .map_err(|error| viewer_failed("the screen's viewer cannot start", &error))?;
        Ok(ScreenComposer {
            run,
            delivered,
            ui: PlayerUi::default(),
            sprites: SpriteTextures::default(),
            textures: PlayerTextures::default(),
            size: self.size,
        })
    }
}

impl DeviceSource for ScreenDevice {
    fn adapter(&mut self) -> Result<wgpu::Adapter, String> {
        self.gpu.adapter().map_err(|error| error.to_string())
    }

    fn device(
        &mut self,
        requirements: &DeviceRequirements,
    ) -> Result<(wgpu::Device, SubmissionAuthority), String> {
        self.gpu
            .device(&self.options, requirements)
            .map_err(|error| error.to_string())
    }
}

type Delivered = Rc<RefCell<Vec<(u64, RetainedCaptureOutcome)>>>;

/// The session's driver: it owns no game state and keeps the captures the session delivers.
#[derive(Default)]
pub(crate) struct CaptureSink {
    delivered: Delivered,
}

impl RetainedDriver for CaptureSink {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        Ok(())
    }

    fn event(
        &mut self,
        _: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        if let RetainedEvent::Capture { id, outcome, .. } = event {
            self.delivered.borrow_mut().push((*id, outcome.clone()));
        }
        Ok(())
    }
}

/// A windowless viewer session on the run's device, and the UI state a window keeps for its
/// frames.
pub(crate) struct ScreenComposer<'a> {
    run: WindowlessRetained<'a>,
    delivered: Delivered,
    ui: PlayerUi,
    sprites: SpriteTextures,
    textures: PlayerTextures,
    size: [u32; 2],
}

impl ScreenComposer<'_> {
    /// Composes the frame the world holds now, the screen with the UI over it, and writes it as an
    /// RGB PNG to each of `files`. `drawn` is the frame whose work drew the screen, which the log
    /// names.
    ///
    /// # Errors
    /// Fails when the world's UI cannot be read or drawn, when the screen texture cannot be shown,
    /// when the viewer's capture fails, or when a file cannot be written.
    pub(crate) fn write(
        &mut self,
        host: &DisunityHost,
        gpu: &PlayerGpu,
        drawn: u64,
        files: &[&Path],
    ) -> Result<(), PlayError> {
        let frame = self.compose(host, gpu, drawn)?;
        for file in files {
            frame.write_png(file)?;
            eprintln!(
                "[player] screen of frame {drawn} written to {}",
                file.display()
            );
        }
        Ok(())
    }

    fn compose(
        &mut self,
        host: &DisunityHost,
        gpu: &PlayerGpu,
        drawn: u64,
    ) -> Result<Capture, PlayError> {
        self.send_ui(host, gpu)?;
        let screen = gpu.written_screen().ok_or_else(|| {
            PlayError::Invalid(format!(
                "the screen drawn by frame {drawn} cannot be shown with the UI"
            ))
        })?;
        self.run.host_mut().show_game_screen(Some(screen));
        let generation = self.run.host().accepted_generation();
        let id = self
            .run
            .host_mut()
            .request_capture(generation)
            .map_err(|error| PlayError::Invalid(format!("the screen capture: {error}")))?;
        let outcome = self.deliver(id)?;
        let RetainedCaptureOutcome::Completed { image, .. } = outcome else {
            return Err(PlayError::Invalid(
                "the screen capture was cancelled before it completed".into(),
            ));
        };
        let size = [image.viewport.width, image.viewport.height];
        if size != self.size {
            return Err(PlayError::Invalid(format!(
                "the composed screen is {}x{}, not the {}x{} asked for",
                size[0], size[1], self.size[0], self.size[1]
            )));
        }
        Ok(Capture {
            frame: drawn,
            width: size[0],
            height: size[1],
            pixels: image.pixels,
        })
    }

    /// Sends the viewer the UI the world holds, as a window does: the draws the player can show,
    /// with the textures they name converted first.
    fn send_ui(&mut self, host: &DisunityHost, gpu: &PlayerGpu) -> Result<(), PlayError> {
        let mut plan = host.ui_presentation()?;
        self.textures
            .refresh(host, graphic_textures(&plan), &mut self.sprites)?;
        let targets = self.ui.offer_targets(
            &mut plan,
            |plan| gpu.ui_offers(host, plan),
            |message| eprintln!("{message}"),
        );
        self.run.host_mut().show_ui_textures(targets);
        let update = self
            .ui
            .update(plan, self.size, &self.sprites, &self.textures, |message| {
                eprintln!("{message}");
            })?;
        if let Some(update) = update {
            self.run
                .host_mut()
                .apply(update)
                .map_err(|error| viewer_failed("the viewer rejects the UI", &error))?;
        }
        Ok(())
    }

    /// Runs one frame and polls until the capture `id` arrives.
    fn deliver(&mut self, id: u64) -> Result<RetainedCaptureOutcome, PlayError> {
        let deadline = Instant::now() + COMPOSE_TIMEOUT;
        let mut submitted = false;
        loop {
            if !submitted {
                submitted = self
                    .run
                    .frame()
                    .map_err(|error| viewer_failed("the screen's frame", &error))?;
            }
            self.run
                .poll()
                .map_err(|error| viewer_failed("the screen's frame", &error))?;
            let mut delivered = self.delivered.borrow_mut();
            if let Some(at) = delivered.iter().position(|(arrived, _)| *arrived == id) {
                return Ok(delivered.swap_remove(at).1);
            }
            drop(delivered);
            if Instant::now() > deadline {
                return Err(PlayError::Invalid(format!(
                    "the screen capture did not arrive in {} s",
                    COMPOSE_TIMEOUT.as_secs()
                )));
            }
            std::thread::yield_now();
        }
    }
}
