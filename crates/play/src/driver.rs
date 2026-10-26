//! Runs a loaded game in the desktop host.
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_plan::simulation::{SpriteTextures, from_presentation};
use worldrunner_render_scene::{Viewport, simulation::WorldSceneContribution};
use worldrunner_scene_viewer::{
    LiveDriver, LiveFrame, LiveInputEvent, LiveInputSample, LiveReceipt, LiveTick,
};

use crate::{PlayError, Prepared, keys::key_code};

/// Glyph coverage textures the presentation creates take identities from here up.
const FIRST_COVERAGE_TEXTURE: u64 = 1 << 32;

/// One presented frame as the window showed it: tightly packed sRGB RGBA rows, top first.
#[derive(Clone, Debug)]
pub struct Capture {
    pub frame: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Capture {
    /// The RGBA bytes of the pixel at column `x`, row `y` from the top.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[at],
            self.pixels[at + 1],
            self.pixels[at + 2],
            self.pixels[at + 3],
        ]
    }

    /// The colour the window showed at column `x`, row `y` from the top. The window is opaque,
    /// so the alpha the GPU wrote is not part of it.
    #[must_use]
    pub fn rgb(&self, x: u32, y: u32) -> [u8; 3] {
        let [red, green, blue, _] = self.pixel(x, y);
        [red, green, blue]
    }

    /// Writes the frame as an RGB PNG. The window is opaque, so the alpha the GPU wrote
    /// (zero where a camera's background colour has zero alpha) is not part of the picture.
    pub fn write_png(&self, path: &Path) -> Result<(), PlayError> {
        let io = |error| PlayError::Io(path.to_owned(), error);
        let file = std::fs::File::create(path).map_err(io)?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let rgb: Vec<u8> = self
            .pixels
            .chunks_exact(4)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect();
        encoder
            .write_header()
            .and_then(|mut writer| writer.write_image_data(&rgb))
            .map_err(|error| PlayError::Invalid(format!("{}: {error}", path.display())))
    }
}

/// A booted game. Window keys reach the game's `Input` at the simulated time they arrive;
/// each update advances the world by the host's delta and renders its presentation.
pub struct PlayDriver {
    host: DisunityHost,
    textures: SpriteTextures,
    simulated: Duration,
    capture_frames: BTreeSet<u64>,
    captures: Vec<Capture>,
}

impl PlayDriver {
    /// Boots the managed host and loads the prepared scene. Frames in `capture_frames`
    /// are kept as [`Capture`]s.
    pub fn new(
        prepared: Prepared,
        capture_frames: impl IntoIterator<Item = u64>,
    ) -> Result<Self, PlayError> {
        let mut host = DisunityHost::boot(&prepared.config)?;
        host.run_slice(&prepared.request)?;
        Ok(Self {
            host,
            textures: prepared.textures,
            simulated: Duration::ZERO,
            capture_frames: capture_frames.into_iter().collect(),
            captures: Vec::new(),
        })
    }

    #[must_use]
    pub fn captures(&self) -> &[Capture] {
        &self.captures
    }

    fn frame(&mut self, viewport: Viewport) -> Result<LiveFrame, PlayError> {
        let presentation = self
            .host
            .export_presentation([viewport.width, viewport.height], FIRST_COVERAGE_TEXTURE)?;
        let plan = from_presentation(presentation, &self.textures);
        let contribution = WorldSceneContribution::new(plan)
            .map_err(|error| PlayError::Invalid(format!("the scene cannot be drawn: {error}")))?;
        Ok(LiveFrame::from_world(contribution))
    }
}

impl Drop for PlayDriver {
    fn drop(&mut self) {
        if let Err(error) = self.host.unload_slice() {
            eprintln!("unloading the game failed: {error}");
        }
    }
}

impl LiveDriver for PlayDriver {
    fn ingress(&mut self, sample: &LiveInputSample) -> Result<(), String> {
        let LiveInputEvent::Key {
            key,
            pressed,
            repeat: false,
        } = &sample.event
        else {
            return Ok(());
        };
        let Some(code) = key_code(key) else {
            return Ok(());
        };
        // Input must follow the last advanced frame; the next update reads it.
        let at = self.simulated + Duration::from_nanos(1);
        self.host
            .push_key(at, code, *pressed)
            .map_err(|error| error.to_string())
    }

    fn update(&mut self, tick: &LiveTick) -> Result<LiveFrame, String> {
        let nanos =
            u64::try_from(tick.dt.as_nanos()).map_err(|_| "frame delta too long".to_owned())?;
        self.host
            .advance(nanos, 1_000_000_000)
            .map_err(|error| error.to_string())?;
        self.simulated += tick.dt;
        let mut frame = self
            .frame(tick.viewport)
            .map_err(|error| error.to_string())?;
        if self.capture_frames.contains(&tick.frame) {
            frame.capture = Some(tick.frame);
        }
        Ok(frame)
    }

    fn presented(&mut self, receipt: LiveReceipt) -> Result<(), String> {
        if let Some(capture) = receipt.capture {
            self.captures.push(Capture {
                frame: capture.id,
                width: capture.image.viewport.width,
                height: capture.image.viewport.height,
                pixels: capture.image.pixels,
            });
        }
        Ok(())
    }
}
