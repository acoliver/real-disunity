//! Plays a built player's scene in the scene viewer's retained path, which draws 3D objects.
//!
//! Each update advances the game, reads the mesh renderers its frame shows and turns them into
//! retained edits (see [`SceneTables`]). The first frame that shows a mesh object sets the
//! viewer's camera to the game's camera; after that the window's pointer and keys orbit it
//! ([`Navigation`]) while the keys also reach the game's `Input`. Screen-space UI uses the
//! engine overlay; ordinary 2D scene quads remain outside this retained path.

#[cfg(test)]
#[path = "retained_ui_tests.rs"]
mod ui_tests;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use worldrunner_camera::{CameraState, ViewConvention, ViewHandedness};
use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_plan::{
    SceneCamera, SceneMesh, SceneTexture, SceneValidationError, simulation::SpriteTextures,
};
use worldrunner_render_scene::CameraPose;
use worldrunner_scene_viewer::{
    CapturePoolPolicy, FrameProfile, PresentMode, PresentationBudget, PresentationCadence,
    RenderClock, RetainedCaptureOutcome, RetainedDriver, RetainedEvent, RetainedEventHost,
    RetainedHost, RetainedInput, RetainedTick, RetainedUpdate, ViewerOptions, WindowSizeUnit,
};

use crate::{
    PlayError, PreparedPlayer,
    driver::{Capture, FIRST_COVERAGE_TEXTURE},
    keys::key_code,
    navigation::Navigation,
    player_meshes::PlayerMeshes,
    player_textures::PlayerTextures,
    player_ui::PlayerUi,
    scene_tables::{Changes, FrameContent, FrameUpdate, SceneTables, Skipped},
    scene_transform::{orbit_limits, scene_camera},
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
/// frames are presented, and returns the captured frames.
pub fn play_player_window(
    prepared: &PreparedPlayer,
    window: PlayerWindow,
) -> Result<Vec<Capture>, PlayError> {
    let mut host = DisunityHost::boot(&prepared.config)?;
    host.run_slice(&prepared.request)?;
    let mut driver = PlayerRetainedDriver::new(host, window)?;
    let scene =
        SceneTables::initial_scene().map_err(|error| scene_error("the initial scene", &error))?;
    let viewer = RetainedHost::new(scene, viewer_options(&driver.window))
        .map_err(|error| PlayError::Invalid(format!("the viewer cannot start: {error}")))?;
    viewer
        .run(&mut driver)
        .map_err(|error| PlayError::Invalid(error.to_string()))?;
    Ok(std::mem::take(&mut driver.captures))
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
        adapter_callback: None,
        cadence: PresentationCadence::default(),
        render_clock: RenderClock::default(),
    }
}

/// Reads the scene contents of a frame from the converted meshes and textures.
struct Content<'a> {
    meshes: &'a PlayerMeshes,
    textures: &'a SpriteTextures,
}

impl FrameContent for Content<'_> {
    fn mesh(&self, identity: u64) -> Option<&SceneMesh> {
        self.meshes.get(identity).map(|held| &held.mesh)
    }

    fn texture(&self, identity: u64) -> Option<&SceneTexture> {
        self.textures.textures.get(&identity)
    }
}

struct PlayerRetainedDriver {
    host: DisunityHost,
    window: PlayerWindow,
    tables: SceneTables,
    meshes: PlayerMeshes,
    player_textures: PlayerTextures,
    ui: PlayerUi,
    sprites: SpriteTextures,
    navigation: Navigation,
    lens: SceneCamera,
    camera_set: bool,
    simulated: Duration,
    size: [u32; 2],
    presented: u64,
    reported: BTreeSet<u64>,
    /// Requested captures not delivered yet, by request id, with the update they belong to.
    pending: BTreeMap<u64, u64>,
    captures: Vec<Capture>,
}

impl PlayerRetainedDriver {
    fn new(host: DisunityHost, window: PlayerWindow) -> Result<Self, PlayError> {
        Ok(Self {
            host,
            tables: SceneTables::new().map_err(|error| scene_error("the tables", &error))?,
            meshes: PlayerMeshes::default(),
            player_textures: PlayerTextures::default(),
            ui: PlayerUi::default(),
            sprites: SpriteTextures::default(),
            navigation: Navigation::default(),
            lens: viewer_options(&window).camera,
            camera_set: false,
            simulated: Duration::ZERO,
            size: window.size,
            presented: 0,
            reported: BTreeSet::new(),
            pending: BTreeMap::new(),
            captures: Vec::new(),
            window,
        })
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
        self.host.advance(nanos, 1_000_000_000)?;
        self.simulated += dt;
        self.size = viewer.size().map(|side| side.max(1));
        let presentation = self
            .host
            .export_presentation(self.size, FIRST_COVERAGE_TEXTURE)?;
        let objects = self.host.mesh_objects()?;
        let ui = self.host.ui_presentation()?;
        self.refresh(&objects, &ui)?;
        let changes = Changes {
            meshes: self.meshes.take_changed(),
            textures: self.player_textures.take_changed(),
        };
        let content = Content {
            meshes: &self.meshes,
            textures: &self.sprites,
        };
        let update = self.tables.update(&objects, &content, &changes);
        self.report(&update.skipped);
        apply(viewer, update)?;
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
        self.camera(viewer, presentation.camera_state, !objects.is_empty())?;
        self.capture(viewer, tick.frame)
    }

    fn refresh(
        &mut self,
        objects: &[worldrunner_disunity_runtime::MeshObjectView],
        ui: &worldrunner_render_plan::ui::UiPresentation,
    ) -> Result<(), PlayError> {
        let images = ui
            .draws
            .iter()
            .filter(|draw| draw.kind == worldrunner_render_plan::ui::UiDrawKind::Graphic)
            .filter_map(|draw| draw.texture);
        let shown = objects
            .iter()
            .filter_map(|object| object.slots.first())
            .map(|slot| slot.texture)
            .filter(|texture| *texture != 0)
            .chain(images);
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

    /// Points the viewer at the game's camera once the first mesh object shows, then sends the
    /// pose the navigation moves it to.
    fn camera(
        &mut self,
        viewer: &mut RetainedHost,
        game: Option<CameraState>,
        shows_objects: bool,
    ) -> Result<(), PlayError> {
        let pose = if !self.camera_set && shows_objects {
            self.camera_set = true;
            self.first_pose(game)
        } else {
            self.navigation.take_moved()
        };
        let Some(pose) = pose else {
            return Ok(());
        };
        viewer
            .apply(RetainedUpdate::Camera {
                pose,
                lens: self.lens,
                convention: LEFT_HANDED,
            })
            .map_err(|error| PlayError::Invalid(format!("the camera is not accepted: {error}")))
    }

    fn first_pose(&mut self, game: Option<CameraState>) -> Option<CameraPose> {
        let Some(state) = game else {
            eprintln!("the game presents no camera; the window keeps its default view");
            return None;
        };
        match scene_camera(&state) {
            Ok((pose, lens)) => {
                self.lens = lens;
                if let Err(error) = self.navigation.start(pose, orbit_limits()) {
                    eprintln!("the camera cannot be orbited: {error}");
                }
                Some(pose)
            }
            Err(error) => {
                eprintln!("the game's camera is not shown: {error}");
                None
            }
        }
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
            RetainedEvent::Shutdown | RetainedEvent::WorkReady | RetainedEvent::Timer { .. } => {}
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
