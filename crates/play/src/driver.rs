//! Runs a loaded original game in the retained host's window.
use std::path::Path;
use std::sync::Arc;

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_plan::simulation::SpriteTextures;
use worldrunner_render_scene::simulation::WorldSceneContribution;
use worldrunner_scene_viewer::{
    RetainedDriver, RetainedEvent, RetainedEventHost, RetainedHost, RetainedTick, RetainedUpdate,
    ViewerStart,
};

use crate::{
    PlayError, PlayerWindow, Prepared,
    player_meshes::PlayerMeshes,
    player_textures::PlayerTextures,
    retained_driver::viewer_options,
    scene_tables::SceneTables,
    unresolved::UnresolvedTextures,
    window_script::{Turn, WindowScript},
};

/// Glyph coverage textures the presentation creates take identities from here up.
pub(crate) const FIRST_COVERAGE_TEXTURE: u64 = 1 << 32;

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
        write_rgb_png(path, [self.width, self.height], &self.pixels)
    }
}

/// Writes `rgba`, tightly packed rows of four 8-bit channels, top row first, as an RGB PNG of
/// `size`: the alpha is not part of the picture.
pub(crate) fn write_rgb_png(path: &Path, size: [u32; 2], rgba: &[u8]) -> Result<(), PlayError> {
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    write_png_bytes(path, size, png::ColorType::Rgb, &rgb)
}

/// Writes `rgba`, tightly packed rows of four 8-bit channels, top row first, as an RGBA PNG of
/// `size`, keeping the alpha.
pub(crate) fn write_rgba_png(path: &Path, size: [u32; 2], rgba: &[u8]) -> Result<(), PlayError> {
    write_png_bytes(path, size, png::ColorType::Rgba, rgba)
}

/// Writes `data`, the 8-bit channels of `color` in rows from the top, as a PNG of `size`.
fn write_png_bytes(
    path: &Path,
    size: [u32; 2],
    color: png::ColorType,
    data: &[u8],
) -> Result<(), PlayError> {
    let io = |error| PlayError::Io(path.to_owned(), error);
    let file = std::fs::File::create(path).map_err(io)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size[0], size[1]);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(data))
        .map_err(|error| PlayError::Invalid(format!("{}: {error}", path.display())))
}

/// A booted original game. Window keys reach the game's `Input` at the simulated time they
/// arrive; each update advances the world and shows its presentation as one world frame.
struct PlayDriver {
    host: DisunityHost,
    textures: SpriteTextures,
    player_textures: PlayerTextures,
    player_meshes: PlayerMeshes,
    unresolved: UnresolvedTextures,
    script: WindowScript,
}

/// Boots the game `prepared` names and plays it in a window until the game has advanced
/// `window.max_frames` frames or the window closes, returning the frames `window.capture_frames`
/// names. The window's keys reach the game at their frames, and each frame advances the game by
/// `window.fixed_dt` or by the time the window took.
///
/// # Errors
/// Fails when the window asks for a frame it never plays, the game cannot boot or load its scene,
/// the viewer cannot start or a frame cannot be drawn.
pub fn play_game_window(
    prepared: Prepared,
    window: &PlayerWindow,
) -> Result<Vec<Capture>, PlayError> {
    window.validate()?;
    let mut host = DisunityHost::boot(&prepared.config)?;
    host.run_slice(&prepared.request)?;
    let mut driver = PlayDriver {
        host,
        textures: prepared.textures,
        player_textures: PlayerTextures::default(),
        player_meshes: PlayerMeshes::default(),
        unresolved: UnresolvedTextures::default(),
        script: WindowScript::new(window),
    };
    let scene = SceneTables::initial_scene(None).map_err(|error| {
        PlayError::Invalid(format!("the initial scene cannot be drawn: {error}"))
    })?;
    let start = ViewerStart::new()
        .map_err(|error| PlayError::Invalid(format!("the viewer cannot start: {error}")))?;
    let viewer = RetainedHost::new(scene, viewer_options(window))
        .map_err(|error| PlayError::Invalid(format!("the viewer cannot start: {error}")))?;
    viewer
        .run(start, &mut driver)
        .map_err(|error| PlayError::Invalid(error.to_string()))?;
    driver.script.finish()
}

impl PlayDriver {
    /// Advances the game and shows the frame it presents.
    fn step(&mut self, viewer: &mut RetainedHost, tick: &RetainedTick) -> Result<(), PlayError> {
        let turn = self.script.next(tick);
        if turn == Turn::Held {
            return Ok(());
        }
        self.script.advance(&mut self.host, &turn)?;
        let frame = self.frame(viewer.size())?;
        viewer
            .apply(RetainedUpdate::World(Arc::new(frame)))
            .map_err(|error| {
                PlayError::Invalid(format!("the viewer rejects the frame: {error}"))
            })?;
        self.script.request_capture(viewer)
    }

    fn frame(&mut self, size: [u32; 2]) -> Result<WorldSceneContribution, PlayError> {
        let mut plan = self
            .host
            .export_presentation(size, FIRST_COVERAGE_TEXTURE)?;
        self.player_textures.refresh(
            &self.host,
            plan.quads.iter().filter_map(|quad| quad.texture),
            &mut self.textures,
        )?;
        let mesh_objects = self.host.mesh_objects()?;
        self.player_meshes.refresh(&self.host, &mesh_objects)?;
        // A texture with no texels to draw (an unsupported player format, say) leaves its sprites
        // out of the frame, each texture named once on stderr.
        for id in self
            .unresolved
            .drop_unresolved(&mut plan, &self.textures.textures)
        {
            eprintln!(
                "a sprite shows texture {id}, which has no texels to draw; its sprites are left out"
            );
        }
        WorldSceneContribution::new(plan, &self.textures.textures)
            .map_err(|error| PlayError::Invalid(format!("the scene cannot be drawn: {error}")))
    }
}

impl Drop for PlayDriver {
    fn drop(&mut self) {
        if let Err(error) = self.host.unload_slice() {
            eprintln!("unloading the game failed: {error}");
        }
    }
}

impl RetainedDriver for PlayDriver {
    fn update(&mut self, viewer: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String> {
        self.step(viewer, tick).map_err(|error| error.to_string())
    }

    fn event(
        &mut self,
        viewer: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        self.script.event(&mut self.host, viewer, event)
    }
}
