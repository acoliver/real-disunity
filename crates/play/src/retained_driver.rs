//! Plays a built player's scene in the scene viewer's retained path, which draws 3D objects.
//!
//! Each update advances the game, reads the mesh renderers its frame shows and turns them into
//! retained edits (see [`SceneTables`]). From the first frame that shows a mesh object the
//! viewer follows the game's camera, sent again whenever its pose or lens changes. Before then
//! the window's pointer and keys orbit the opening view ([`Navigation`]); keys always reach the
//! game's `Input`. Screen-space UI uses the
//! engine overlay; ordinary 2D scene quads remain outside this retained path.

#[cfg(test)]
#[path = "retained_gpu_tests.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "retained_screen_tests.rs"]
mod screen_tests;
#[cfg(test)]
#[path = "retained_ui_tests.rs"]
pub(crate) mod ui_tests;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use worldrunner_camera::{CameraState, ViewConvention, ViewHandedness};
use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_gpu::{device::DeviceRequirements, submission::SubmissionAuthority};
use worldrunner_render_plan::{
    SceneCamera, SceneMesh, SceneTexture, SceneValidationError, simulation::SpriteTextures,
};
use worldrunner_render_scene::CameraPose;
use worldrunner_scene_viewer::{
    CameraContinuity, CapturePoolPolicy, FrameProfile, PresentMode, PresentationBudget,
    PresentationCadence, RenderClock, RetainedCaptureOutcome, RetainedDriver, RetainedEvent,
    RetainedEventHost, RetainedHost, RetainedInput, RetainedTick, RetainedUpdate, ViewerOptions,
    ViewerStart, WindowMode, WindowSizeUnit,
};
use worldrunner_world_simulation::ui::presentation::UiPresentation;

use crate::{
    PlayError, PreparedPlayer,
    camera_start::CameraStart,
    driver::{Capture, FIRST_COVERAGE_TEXTURE},
    gpu_device::DeviceSource,
    gpu_player::PlayerGpu,
    keys::key_code,
    navigation::Navigation,
    player::window_gpu,
    player_meshes::PlayerMeshes,
    player_textures::PlayerTextures,
    player_ui::{PlayerUi, graphic_textures},
    scene_census::WhiteReason,
    scene_light::{CameraPublication, SceneLighting},
    scene_tables::{Changes, FrameContent, FrameUpdate, SceneTables, Skipped},
    scene_transform::orbit_limits,
};

const LEFT_HANDED: ViewConvention = ViewConvention {
    handedness: ViewHandedness::DirectXLeftHanded,
};

/// One key press or release, delivered to the game before the update at `frame`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledKey {
    pub frame: u64,
    pub code: u16,
    pub pressed: bool,
}

/// How a player window runs.
#[derive(Clone, Debug)]
pub struct PlayerWindow {
    pub title: String,
    pub size: [u32; 2],
    /// A fixed game step per update, in place of the time the window took.
    pub fixed_dt: Option<Duration>,
    /// The window closes after this many updates are presented.
    pub max_frames: Option<u64>,
    /// The updates whose presented frame is kept as a [`Capture`].
    pub capture_frames: BTreeSet<u64>,
    pub keys: Vec<ScheduledKey>,
}

/// Boots the player, shows its scene in a window until the window closes or `max_frames`
/// frames are presented, and returns the captured frames. The window does the player's offscreen
/// work as a headless run does: a script's synchronous GPU flush, such as a
/// `Texture2D.ReadPixels` after a `Graphics.Blit`, is answered, and each update's offscreen work
/// runs on the viewer's own device, in the viewer's frame.
///
/// # Errors
/// Fails naming the reason when the viewer's device cannot be had, and when the scene cannot start
/// or the window fails.
pub fn play_player_window(
    prepared: &PreparedPlayer,
    window: PlayerWindow,
) -> Result<Vec<Capture>, PlayError> {
    let mut start = ViewerStart::new()
        .map_err(|error| PlayError::Invalid(format!("the viewer cannot start: {error}")))?;
    let options = viewer_options(&window);
    let gpu = window_gpu(
        &prepared.data,
        &prepared.gpu,
        &mut ViewerDevice {
            start: &mut start,
            options: &options,
        },
    )?;
    let (viewer, mut driver) = start_window(prepared, window, gpu)?;
    let ran = viewer.run(start, &mut driver);
    driver.report_gpu();
    ran.map_err(|error| PlayError::Invalid(error.to_string()))?;
    Ok(std::mem::take(&mut driver.captures))
}

/// The device the viewer's start makes before the scene loads, as the player's source for the one
/// it shares.
struct ViewerDevice<'a> {
    start: &'a mut ViewerStart,
    options: &'a ViewerOptions,
}

impl DeviceSource for ViewerDevice<'_> {
    fn adapter(&mut self) -> Result<wgpu::Adapter, String> {
        self.start.adapter().map_err(|error| error.to_string())
    }

    fn device(
        &mut self,
        requirements: &DeviceRequirements,
    ) -> Result<(wgpu::Device, SubmissionAuthority), String> {
        self.start
            .device(self.options, requirements)
            .map_err(|error| error.to_string())
    }
}

/// Boots the host, loads the scene and makes the viewer and the driver that plays it. `gpu`
/// answers the scene's GPU flushes from the first one, so it is installed before the scene loads.
fn start_window(
    prepared: &PreparedPlayer,
    window: PlayerWindow,
    gpu: PlayerGpu,
) -> Result<(RetainedHost, PlayerRetainedDriver), PlayError> {
    let mut host = DisunityHost::boot(&prepared.config)?;
    // Installed before the scene loads: a script may read a render texture back while it does.
    // The screen is the window's size from the start, so a script that draws into it while the
    // scene loads draws into a screen of that size.
    gpu.install(&mut host)?;
    gpu.set_screen(Some(window.size));
    host.run_slice(&prepared.request)?;
    let mut options = viewer_options(&window);
    // The first frame's lights decide whether the viewer starts with shadows, which it cannot
    // add later.
    host.export_presentation(window.size, FIRST_COVERAGE_TEXTURE)?;
    let (lighting, shadows) = SceneLighting::start(&host.presentation_lights()?, options.camera)
        .map_err(PlayError::Invalid)?;
    options.shadows = shadows;
    let scene = SceneTables::initial_scene(lighting.effects().cloned())
        .map_err(|error| scene_error("the initial scene", &error))?;
    let viewer = RetainedHost::new(scene, options)
        .map_err(|error| PlayError::Invalid(format!("the viewer cannot start: {error}")))?;
    let driver = PlayerRetainedDriver::new(host, window, lighting, gpu)?;
    Ok((viewer, driver))
}

fn scene_error(what: &str, error: &SceneValidationError) -> PlayError {
    PlayError::Invalid(format!("{what} cannot be drawn: {error}"))
}

/// The viewer's settings before the first mesh object: a view over the origin the window shows
/// until the game's own camera replaces it.
pub(crate) fn viewer_options(window: &PlayerWindow) -> ViewerOptions {
    ViewerOptions {
        activity_reason: worldrunner_scene_viewer::ActivityReason::default(),
        presentation_budget: PresentationBudget::default().into(),
        capture_pool: CapturePoolPolicy::default(),
        window_size_unit: WindowSizeUnit::default(),
        window_mode: WindowMode::default(),
        camera_convention: LEFT_HANDED,
        ssr: None,
        title: window.title.clone(),
        size: window.size,
        pose: CameraPose {
            eye_millimeters: [0, -10_000, 5_000],
            target_millimeters: [0, 0, 0],
        },
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 10_000_000,
        },
        orbit: orbit_limits(),
        shadows: None,
        frame_profile: FrameProfile::Legacy,
        frames_in_flight: 2,
        gpu_timing: false,
        present_mode: PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        texture_tiers: None,
        adapter_callback: None,
        cadence: PresentationCadence::default(),
        render_clock: RenderClock::default(),
        game: worldrunner_scene_viewer::GameRendering::default(),
    }
}

/// Reads the scene contents of a frame from the converted meshes and textures.
struct Content<'a> {
    meshes: &'a PlayerMeshes,
    textures: &'a SpriteTextures,
    /// Which textures the managed host stored but could not convert.
    player_textures: &'a PlayerTextures,
}

impl FrameContent for Content<'_> {
    fn mesh(&self, identity: u64) -> Option<&SceneMesh> {
        self.meshes.get(identity).map(|held| &held.mesh)
    }

    fn texture(&self, identity: u64) -> Option<&SceneTexture> {
        self.textures.textures.get(&identity)
    }

    fn missing_texture(&self, identity: u64) -> WhiteReason {
        if self.player_textures.conversion_failed(identity) {
            WhiteReason::ConversionFailed
        } else {
            WhiteReason::TexelsNotStored
        }
    }
}

struct PlayerRetainedDriver {
    host: DisunityHost,
    /// The device the window's offscreen work runs on. Declared after `host` so the host is
    /// dropped first.
    gpu: PlayerGpu,
    window: PlayerWindow,
    tables: SceneTables,
    meshes: PlayerMeshes,
    player_textures: PlayerTextures,
    ui: PlayerUi,
    sprites: SpriteTextures,
    navigation: Navigation,
    lighting: SceneLighting,
    lens: SceneCamera,
    /// The game's camera the window follows.
    start: CameraStart,
    /// The camera last accepted by the viewer, from the pose it opens with.
    publication: CameraPublication,
    simulated: Duration,
    size: [u32; 2],
    presented: u64,
    reported: BTreeSet<u64>,
    /// Requested captures not delivered yet, by request id, with the update they belong to.
    pending: BTreeMap<u64, u64>,
    captures: Vec<Capture>,
}

impl PlayerRetainedDriver {
    fn new(
        host: DisunityHost,
        window: PlayerWindow,
        lighting: SceneLighting,
        gpu: PlayerGpu,
    ) -> Result<Self, PlayError> {
        // The opening view can be orbited until the game presents a camera, which then owns it.
        let mut navigation = Navigation::default();
        navigation
            .start(viewer_options(&window).pose, orbit_limits())
            .map_err(PlayError::Invalid)?;
        Ok(Self {
            host,
            gpu,
            tables: SceneTables::new().map_err(|error| scene_error("the tables", &error))?,
            meshes: PlayerMeshes::default(),
            player_textures: PlayerTextures::default(),
            ui: PlayerUi::default(),
            sprites: SpriteTextures::default(),
            navigation,
            lighting,
            lens: viewer_options(&window).camera,
            start: CameraStart::default(),
            publication: CameraPublication::new(viewer_options(&window).pose),
            simulated: Duration::ZERO,
            size: window.size,
            presented: 0,
            reported: BTreeSet::new(),
            pending: BTreeMap::new(),
            captures: Vec::new(),
            window,
        })
    }

    /// Prints the GPU report of the run, as a headless run does when it ends, with the census of
    /// the renderers drawn white.
    fn report_gpu(&self) {
        self.gpu.note_retained(self.tables.census().clone());
        self.gpu.report().print(None);
    }

    fn push_key(&mut self, code: u16, pressed: bool) -> Result<(), PlayError> {
        // Input must follow the last advanced frame; the next update reads it.
        let at = self.simulated + Duration::from_nanos(1);
        Ok(self.host.push_key(at, code, pressed)?)
    }

    /// Advances the game and brings the viewer's scene to the frame it shows.
    fn step(&mut self, viewer: &mut RetainedHost, tick: &RetainedTick) -> Result<(), PlayError> {
        let due: Vec<ScheduledKey> = self
            .window
            .keys
            .iter()
            .copied()
            .filter(|key| key.frame == tick.frame)
            .collect();
        for key in due {
            self.push_key(key.code, key.pressed)?;
        }
        let dt = self.window.fixed_dt.unwrap_or(if tick.frame == 0 {
            Duration::ZERO
        } else {
            tick.dt
        });
        let nanos = u64::try_from(dt.as_nanos())
            .map_err(|_| PlayError::Invalid("frame delta too long".into()))?;
        self.resize_screen(viewer.size());
        self.host.advance(nanos, 1_000_000_000)?;
        self.simulated += dt;
        let presentation = self
            .host
            .export_presentation(self.size, FIRST_COVERAGE_TEXTURE)?;
        let objects = self.host.mesh_objects()?;
        let mut ui = self.host.ui_presentation()?;
        self.refresh(&objects, &ui)?;
        // After the meshes are refreshed, as in a headless run, and before the frame's textures
        // and UI go to the viewer, so texels a readback filled this frame show this frame.
        self.show_screen(viewer)?;
        let changes = Changes {
            meshes: self.meshes.take_changed(),
            textures: self.player_textures.take_changed(),
        };
        let content = Content {
            meshes: &self.meshes,
            textures: &self.sprites,
            player_textures: &self.player_textures,
        };
        let update = self.tables.update(&objects, &content, &changes);
        self.report(&update.skipped);
        apply(viewer, update)?;
        self.offer_ui_textures(viewer, &mut ui);
        if let Some(update) = self.ui.update(
            ui,
            self.size,
            &self.sprites,
            &self.player_textures,
            |message| {
                eprintln!("{message}");
            },
        )? {
            viewer
                .apply(update)
                .map_err(|error| PlayError::Invalid(error.to_string()))?;
        }
        self.light(viewer)?;
        self.camera(viewer, presentation.camera_state, !objects.is_empty())?;
        self.capture(viewer, tick.frame)
    }

    fn resize_screen(&mut self, size: [u32; 2]) {
        self.size = size.map(|side| side.max(1));
        self.gpu.set_screen(Some(self.size));
    }

    /// Hands the frame's offscreen work to the viewer, which runs it in its next frame, and
    /// offers the game's own frame (the screen texture) for the viewer to show in place of the
    /// stock scene. The viewer shows it once a frame it submitted wrote it; no frame is read back.
    /// The offer is made every update so a texture a resize replaced or a screen taken away is
    /// withdrawn at once.
    fn show_screen(&mut self, viewer: &mut RetainedHost) -> Result<(), PlayError> {
        if let Some(operations) = self.gpu.frame_for_viewer(&mut self.host)? {
            viewer.admit_offscreen(operations);
        }
        viewer.show_game_screen(self.gpu.game_screen());
        Ok(())
    }

    /// Offers the viewer the render textures the UI's draws read, as the player's device holds them
    /// now, and takes out the draws whose texture cannot be shown.
    fn offer_ui_textures(&mut self, viewer: &mut RetainedHost, ui: &mut UiPresentation) {
        let offers = self.gpu.ui_offers(&self.host, ui);
        let targets = self
            .ui
            .offer_targets(ui, offers, |message| eprintln!("{message}"));
        viewer.show_ui_textures(targets);
    }

    /// Sends the viewer new scene effects when the frame's lighting changed.
    fn light(&mut self, viewer: &mut RetainedHost) -> Result<(), PlayError> {
        if let Some(effects) = self
            .lighting
            .update(&self.host.presentation_lights()?)
            .map_err(PlayError::Invalid)?
        {
            viewer
                .apply(RetainedUpdate::Settings(Box::new(effects)))
                .map_err(|error| PlayError::Invalid(error.to_string()))?;
        }
        Ok(())
    }

    fn refresh(
        &mut self,
        objects: &[worldrunner_disunity_runtime::MeshObjectView],
        ui: &worldrunner_world_simulation::ui::presentation::UiPresentation,
    ) -> Result<(), PlayError> {
        let shown = objects
            .iter()
            .filter_map(|object| object.slots.first())
            .map(|slot| slot.texture)
            .filter(|texture| *texture != 0)
            .chain(graphic_textures(ui));
        self.player_textures
            .refresh(&self.host, shown, &mut self.sprites)?;
        self.meshes.refresh(&self.host, objects)
    }

    fn report(&mut self, skipped: &[Skipped]) {
        for skip in skipped {
            if self.reported.insert(skip.object) {
                eprintln!(
                    "object {} is left out of the window: {}",
                    skip.object, skip.reason
                );
            }
        }
    }

    /// Points the viewer at the game's camera on every frame that shows objects and presents a
    /// changed camera. Navigation moves only the opening view, before the game presents one.
    fn camera(
        &mut self,
        viewer: &mut RetainedHost,
        game: Option<CameraState>,
        shows_objects: bool,
    ) -> Result<(), PlayError> {
        let step = self.start.next(shows_objects, game.as_ref());
        if let Some(note) = step.note {
            eprintln!("{note}");
        }
        let moved = match step.start {
            Some((pose, lens)) => {
                self.lens = lens;
                Some(pose)
            }
            None if self.start.following() => None,
            None => self.navigation.take_moved(),
        };
        let render_shadows = self.host.presentation_render_shadows()?;
        let lens = self.lens;
        self.publication
            .publish(&mut self.lighting, moved, render_shadows, |pose, policy| {
                viewer
                    .apply(RetainedUpdate::camera_with_shadow_policy(
                        pose,
                        lens,
                        LEFT_HANDED,
                        CameraContinuity::Continuous,
                        policy,
                    ))
                    .map_err(|error| {
                        PlayError::Invalid(format!("the camera is not accepted: {error}"))
                    })
            })
    }

    fn capture(&mut self, viewer: &mut RetainedHost, frame: u64) -> Result<(), PlayError> {
        if !self.window.capture_frames.contains(&frame) {
            return Ok(());
        }
        let id = viewer
            .request_capture(viewer.accepted_generation())
            .map_err(|error| PlayError::Invalid(format!("capture {frame}: {error}")))?;
        self.pending.insert(id, frame);
        Ok(())
    }

    fn finished(&self) -> bool {
        self.window
            .max_frames
            .is_some_and(|max| self.presented >= max)
            && self.pending.is_empty()
    }
}

/// Sends the frame's new and changed resources, and then its object edits.
fn apply(viewer: &mut RetainedHost, update: FrameUpdate) -> Result<(), PlayError> {
    for update in update.into_updates() {
        viewer.apply(update).map_err(|error| {
            PlayError::Invalid(format!("the viewer rejects the frame: {error}"))
        })?;
    }
    Ok(())
}

impl Drop for PlayerRetainedDriver {
    fn drop(&mut self) {
        if let Err(error) = self.host.unload_slice() {
            eprintln!("unloading the game failed: {error}");
        }
    }
}

impl RetainedDriver for PlayerRetainedDriver {
    fn update(&mut self, viewer: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String> {
        self.step(viewer, tick).map_err(|error| error.to_string())
    }

    fn event(
        &mut self,
        viewer: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        match event {
            RetainedEvent::Input(input) => {
                self.navigation.accept(input, self.size);
                if let RetainedInput::Key {
                    key,
                    pressed,
                    repeat: false,
                    ..
                } = input
                    && let Some(code) = key_code(key)
                {
                    self.push_key(code, *pressed)
                        .map_err(|error| error.to_string())?;
                }
            }
            RetainedEvent::Presented { frame } => self.presented = frame + 1,
            RetainedEvent::Capture { id, outcome, .. } => self.delivered(*id, outcome),
            RetainedEvent::Shutdown
            | RetainedEvent::WorkReady
            | RetainedEvent::Timer { .. }
            | RetainedEvent::AudioFinished { .. }
            | RetainedEvent::AudioStatusChanged { .. }
            | RetainedEvent::Accessibility(_) => {}
        }
        if self.finished() {
            viewer.request_exit();
        }
        Ok(())
    }
}

impl PlayerRetainedDriver {
    fn delivered(&mut self, id: u64, outcome: &RetainedCaptureOutcome) {
        let Some(frame) = self.pending.remove(&id) else {
            return;
        };
        if let RetainedCaptureOutcome::Completed { image, .. } = outcome {
            self.captures.push(Capture {
                frame,
                width: image.viewport.width,
                height: image.viewport.height,
                pixels: image.pixels.clone(),
            });
        }
    }
}
