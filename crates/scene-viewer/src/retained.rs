//! CPU scene ownership and ordered edits for the shared native viewer.
use crate::{ViewerError, ViewerOptions};
use std::{collections::BTreeMap, time::Duration};
use worldrunner_render_plan::{RenderScene, SceneCamera, SceneEffects, SceneObject};
use worldrunner_render_scene::{CameraPose, SceneView, Viewport};

pub(crate) mod accessibility;
mod startup;
mod timers;
#[cfg(test)]
#[path = "../../world-simulation/tests/ui_font/mod.rs"]
mod ui_font;
pub use timers::{MAX_HOST_TIMERS, TimerError, TimerHandle};
mod work;
pub use work::{
    CancelToken, WorkCompletion, WorkFailure, WorkQueue, WorkQueueError, WorkSubmitError,
    WorkTicket,
};
mod timing;
pub use timing::{
    FrameTiming, GpuTiming, GpuTimingUnavailable, PassId, PassSlot, PassTiming, StageId,
};
mod statistics;
pub use statistics::{
    DrawCount, EffectEnabled, EffectStatistics, PassExecution, PassInstallation, ReceiverBinding,
    ReceiverBindings, ResourceStatistics, RetainedFrameStatistics, SceneStatistics,
    ShadowStatistics, SsaoStatistics, TerrainGeometryStatistics, TerrainLeafIdentity,
    TerrainPageStatistics, TerrainStreamingStatistics,
};
mod decals;
pub use decals::DecalUpdate;
mod event_host;
mod objects;
pub(crate) mod resource_edits;
pub use event_host::RetainedEventHost;
mod windowless;
pub use windowless::WindowlessRetained;
#[cfg(test)]
pub(crate) use windowless::tests::pages::layered_scene;
mod input;
pub use input::text::TextInput;
pub use input::{
    CursorError, CursorGrabMode, KeyCode, LogicalKey, MouseButton, NativeKeyCode, PhysicalKey,
    WheelDelta, WindowControlError,
};
mod capture;
pub use capture::{
    CaptureJitter, CaptureOutput, CaptureRequestError, RetainedCaptureObservation,
    RetainedCaptureOutcome, RetainedCaptureProducts, RetainedCaptureRecipe,
};
pub(crate) use capture::{Captures, ao_format_from_settings};
pub(crate) mod readiness;
pub(crate) mod session;
pub(crate) use session::Session;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod terrain_tests;

#[cfg(test)]
mod batch_tests;
pub use worldrunner_render_plan::{ObjectId, PickHit};
mod picking;
pub use picking::RetainedPickHit;

/// Ordered edits reference the latest accepted mesh and material tables.
#[derive(Clone, Debug)]
pub enum RetainedUpdate {
    /// Replaces screen-space text without rebuilding scene resources. Empty plans clear it.
    Ui(std::sync::Arc<worldrunner_render_scene::simulation::UiOverlay>),
    Decal(DecalUpdate),
    Add {
        id: ObjectId,
        object: SceneObject,
    },
    Remove(ObjectId),
    Replace {
        id: ObjectId,
        object: SceneObject,
    },
    /// Applies the edits in order as one atomic update: one validation pass, one
    /// scene rebuild and one preparation slot, however many edits it holds.
    /// Later edits see the effect of earlier ones, so the last edit to an ID wins
    /// and an `Add` after a `Remove` of the same ID is accepted. If any edit is
    /// invalid, the error names its index (`object edit N`) and the host is
    /// unchanged. An empty batch is accepted without taking a slot.
    Objects(Vec<SceneObjectEdit>),
    Camera {
        pose: CameraPose,
        lens: SceneCamera,
        convention: worldrunner_camera::ViewConvention,
    },
    /// Lighting and post settings, prepared before atomic frame publication.
    /// Water, SSAO and SSR edits require an HDR frame profile.
    Settings(Box<SceneEffects>),
    /// Installs, replaces or removes terrain and its layered material source.
    /// Preparation and publication use the same bounded queue as resource edits.
    Terrain(Option<std::sync::Arc<worldrunner_render_plan::SceneTerrain>>),
    /// Replace resource tables before adding or replacing objects that reference them.
    /// Existing objects and terrain must remain valid against the new tables.
    /// Textures requiring features absent from the initial device are rejected
    /// during publication.
    Resources {
        resources: worldrunner_render_plan::SceneResources,
        meshes: Vec<worldrunner_render_plan::SceneMesh>,
    },
    /// Adds textures, materials and meshes after the resident ones without rebuilding the scene.
    /// The new items take the next handles in each table, and the new materials and later
    /// objects may use old and new items alike. Validation covers only the new items; only
    /// their bytes are prepared, admitted against the presentation budget and uploaded.
    /// Items that depend on state shared by the whole scene (impostors, fade thresholds, terrain
    /// contact, specular cubes) fail with `DrawError::NeedsRebuild` and change nothing.
    AppendResources(worldrunner_render_plan::SceneAppend),
    /// Gives one texture new contents. Handles, materials and objects stay as they are, and only
    /// the materials that sample the texture are rebound.
    ReplaceTexture {
        handle: worldrunner_render_plan::TextureHandle,
        texture: worldrunner_render_plan::SceneTexture,
    },
    /// Gives one mesh new contents. Handles and objects stay as they are.
    ReplaceMesh {
        handle: worldrunner_render_plan::MeshHandle,
        mesh: worldrunner_render_plan::SceneMesh,
    },
}

/// One object edit inside `RetainedUpdate::Objects`, with the semantics of the
/// matching single-object `RetainedUpdate` variant.
#[derive(Clone, Copy, Debug)]
pub enum SceneObjectEdit {
    Add { id: ObjectId, object: SceneObject },
    Replace { id: ObjectId, object: SceneObject },
    Remove(ObjectId),
}

/// Typed OS input, without application action mapping or GPU types.
/// Key names follow `LiveInputEvent`'s lowercase convention.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedInput {
    Text(TextInput),
    DisplayChanged {
        scale_factor: f64,
        physical_size: [u32; 2],
    },
    Key {
        /// Lowercase name retained for held-key queries.
        key: String,
        /// Unmodified logical identity from the OS.
        logical: LogicalKey,
        physical: PhysicalKey,
        pressed: bool,
        repeat: bool,
    },
    /// Position in physical window pixels.
    PointerMoved([f64; 2]),
    /// Cancels active gestures before focus-loss releases are delivered.
    PointerCancelled,
    Button {
        button: MouseButton,
        pressed: bool,
    },
    Wheel(WheelDelta),
    RawMotion([f64; 2]),
    CursorEntered,
    CursorLeft,
    Focused(bool),
    Occluded(bool),
    Resized([u32; 2]),
    SurfaceRecreated,
    CloseRequested,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedEvent {
    /// Route through `UiRouter::accessibility_action`, then handle its ordinary `UiAction`s.
    Accessibility(worldrunner_world_simulation::ui::UiAccessRequest),
    /// CPU results are available. Drain caller-owned queues, even during presentation pauses.
    WorkReady,
    /// A host timer expired, including during presentation pauses.
    Timer {
        handle: TimerHandle,
    },
    /// Final notification, after every outstanding capture has been cancelled.
    Shutdown,
    Input(RetainedInput),
    Capture {
        id: u64,
        generation: u64,
        outcome: RetainedCaptureOutcome,
    },
    /// The update was submitted and presented, without a GPU-completion claim.
    Presented {
        frame: u64,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct RetainedTick {
    pub frame: u64,
    pub elapsed: Duration,
    pub dt: Duration,
}

/// Application callbacks run on the host calling thread. No GPU calls are needed.
pub trait RetainedDriver {
    fn update(&mut self, host: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String>;
    /// Overrides the configured clock for the next frame.
    fn render_clock(&self) -> Option<crate::RenderClock> {
        None
    }
    /// Delivered during presentation pauses too. Host controls apply before returning.
    fn event(
        &mut self,
        _host: &mut RetainedEventHost<'_>,
        _event: &RetainedEvent,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// CPU state is available without starting a display or GPU. Edits become visible
/// in generation order on admitted frames. CPU preparation never waits on the event
/// thread; GPU allocation/upload uses the existing renderer at publication.
pub struct RetainedHost {
    pub(crate) accessibility: accessibility::Accessibility,
    picking: picking::Picking,
    work: work::WorkServices,
    timers: timers::Timers,
    pub(crate) gpu_timing: GpuTiming,
    pub(crate) statistics: Option<RetainedFrameStatistics>,
    pub(crate) captures: capture::Captures,
    pub(crate) capture_pool: u64,
    pub(crate) scene: Option<RenderScene>,
    pub(crate) initial_scene: Option<RenderScene>,
    initial_content: Option<startup::Supplier>,
    pub(crate) readiness: readiness::Readiness,
    pub(crate) options: ViewerOptions,
    objects: BTreeMap<ObjectId, SceneObject>,
    /// `scene`'s object list predates edits to `objects`; see `materialize_objects`.
    objects_stale: bool,
    decals: BTreeMap<u64, worldrunner_render_plan::SceneDecal>,
    input_state: input::InputState,
    size: [u32; 2],
    exit: bool,
    occluded: bool,
}

impl RetainedHost {
    /// Effective startup options, including any selected-adapter frame decision.
    pub fn options(&self) -> &ViewerOptions {
        &self.options
    }

    /// Selects shader time independently of CPU edit generations.
    pub fn set_render_clock(&mut self, clock: crate::RenderClock) {
        self.options.render_clock = clock;
    }

    pub fn objects(&self) -> impl Iterator<Item = (ObjectId, SceneObject)> + '_ {
        self.objects.iter().map(|(&id, &object)| (id, object))
    }

    pub fn object(&self, id: ObjectId) -> Option<SceneObject> {
        self.objects.get(&id).copied()
    }
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
    pub fn request_exit(&mut self) {
        self.exit = true;
    }

    /// Invalid edits fail before changing CPU state or enqueuing GPU work.
    pub fn apply(&mut self, update: RetainedUpdate) -> Result<(), ViewerError> {
        self.require_initial_scene()?;
        if self.preparations_in_flight() == self.readiness.limit {
            return Err(ViewerError::BackPressure {
                limit: self.readiness.limit,
            });
        }
        if resource_edits::is_resource_edit(&update) {
            return self.apply_resource_edit(update);
        }
        if let RetainedUpdate::Decal(edit) = &update {
            return self.apply_decal(edit.clone());
        }
        if let RetainedUpdate::Camera {
            pose,
            lens,
            convention,
        } = &update
        {
            Self::validate_camera(*pose, *lens, *convention)?;
            self.options.camera_convention = *convention;
            self.options.pose = *pose;
            self.options.camera = *lens;
            self.enqueue(update, false);
            return Ok(());
        }
        if let RetainedUpdate::Terrain(terrain) = &update {
            self.materialize_objects()?;
            let scene = self
                .scene()
                .with_terrain(terrain.clone())
                .map_err(|error| ViewerError::Driver(error.to_string()))?;
            crate::validate(&scene, &self.options)?;
            self.scene = Some(scene);
            self.enqueue(update, true);
            return Ok(());
        }
        if matches!(&update, RetainedUpdate::Ui(_)) {
            self.enqueue(update, false);
            return Ok(());
        }
        self.apply_scene_update(update)
    }

    fn apply_scene_update(&mut self, update: RetainedUpdate) -> Result<(), ViewerError> {
        if matches!(&update, RetainedUpdate::Objects(edits) if edits.is_empty()) {
            return Ok(());
        }
        let single = |edit| [edit];
        let edited = match &update {
            RetainedUpdate::Objects(edits) => {
                Some(self.edit_objects(edits).map_err(|(index, error)| {
                    ViewerError::Driver(format!("object edit {index}: {error}"))
                }))
            }
            RetainedUpdate::Add { id, object } => Some(
                self.edit_objects(&single(SceneObjectEdit::Add {
                    id: *id,
                    object: *object,
                }))
                .map_err(|(_, error)| error),
            ),
            RetainedUpdate::Remove(id) => Some(
                self.edit_objects(&single(SceneObjectEdit::Remove(*id)))
                    .map_err(|(_, error)| error),
            ),
            RetainedUpdate::Replace { id, object } => Some(
                self.edit_objects(&single(SceneObjectEdit::Replace {
                    id: *id,
                    object: *object,
                }))
                .map_err(|(_, error)| error),
            ),
            _ => None,
        };
        if let Some(edited) = edited {
            edited?;
            self.enqueue(update, false);
            return Ok(());
        }
        self.apply_scene_replacement(update)
    }

    /// Settings and resource updates, which read or replace the whole scene.
    fn apply_scene_replacement(&mut self, update: RetainedUpdate) -> Result<(), ViewerError> {
        self.materialize_objects()?;
        let mut effects = self.scene().effects().cloned();
        let mut replacement = None;
        let rebuild = match &update {
            RetainedUpdate::Settings(next) => {
                Self::validate_streamed_effects(next, &self.options)?;
                let rebuild = readiness::resource_settings_changed(self.scene().effects(), next);
                effects = Some(*next.clone());
                rebuild
            }
            RetainedUpdate::Resources {
                resources: next,
                meshes: next_meshes,
            } => {
                replacement = Some((next.clone(), next_meshes.clone()));
                true
            }
            _ => unreachable!("object, camera, terrain and decal updates are accepted earlier"),
        };
        let scene = self.edited_scene(replacement, &self.objects, effects)?;
        worldrunner_render_scene::draw::validate_decal_materials(&scene)?;
        self.scene = Some(scene);
        self.enqueue(update, rebuild);
        Ok(())
    }

    fn edited_scene(
        &self,
        replacement: Option<(
            worldrunner_render_plan::SceneResources,
            Vec<worldrunner_render_plan::SceneMesh>,
        )>,
        objects: &BTreeMap<ObjectId, SceneObject>,
        effects: Option<SceneEffects>,
    ) -> Result<RenderScene, ViewerError> {
        if let Some((resources, meshes)) = replacement {
            let scene = self.snapshot(resources, meshes, objects, effects)?;
            crate::validate(&scene, &self.options)?;
            Ok(scene)
        } else {
            self.scene()
                .with_objects_and_effects(objects.values().copied().collect(), effects)
                .map_err(|error| ViewerError::Driver(error.to_string()))
        }
    }

    fn validate_camera(
        pose: CameraPose,
        lens: SceneCamera,
        convention: worldrunner_camera::ViewConvention,
    ) -> Result<(), ViewerError> {
        SceneView::with_convention(
            pose,
            lens,
            Viewport {
                width: 1,
                height: 1,
            },
            convention,
        )?;
        Ok(())
    }

    fn validate_object(&self, object: SceneObject) -> Result<(), ViewerError> {
        object
            .transform()
            .validate()
            .map_err(worldrunner_render_scene::draw::InstanceError::from)?;
        if object.mesh().raw() as usize >= self.scene().geometry().meshes().len()
            || object.material().raw() as usize >= self.scene().resources().materials().len()
        {
            return Err(ViewerError::Driver(
                "object references a nonresident resource".into(),
            ));
        }
        Ok(())
    }

    /// Returns transitions to deliver in order. Focus loss first cancels gestures, then
    /// releases keys in physical-key order and buttons in button order before the focus event.
    /// Repeated focus loss never repeats a release.
    pub fn input(&mut self, input: RetainedInput) -> Vec<RetainedInput> {
        if let RetainedInput::Resized(size) = &input {
            if self.input_state.surface_extent == Some(*size) {
                return Vec::new();
            }
            self.input_state.surface_extent = Some(*size);
        }
        let mut events = self.input_state.accept(&input);
        if let RetainedInput::Resized(physical_size) = &input {
            events.push(RetainedInput::DisplayChanged {
                scale_factor: self.scale_factor(),
                physical_size: *physical_size,
            });
        }
        match &input {
            RetainedInput::Resized(size) => self.size = *size,
            RetainedInput::DisplayChanged {
                scale_factor,
                physical_size,
            } => {
                self.input_state.text.scale = *scale_factor;
                self.size = *physical_size;
            }
            RetainedInput::Focused(false) => {
                self.input_state.text.area = None;
                self.apply_text_input_area();
            }
            RetainedInput::Occluded(value) => self.occluded = *value,
            RetainedInput::CloseRequested => self.exit = true,
            _ => {}
        }
        events.push(input);
        events
    }

    /// Whether a frame can be drawn now: a scene is staged and the surface is visible and
    /// nonempty. This is the condition under which [`Self::view`] returns a view.
    pub(crate) fn has_view_extent(&self) -> bool {
        self.initial_scene.is_some() && !self.size.contains(&0) && !self.occluded
    }

    pub fn view(&self) -> Result<Option<SceneView>, ViewerError> {
        if !self.has_view_extent() {
            return Ok(None);
        }
        Ok(Some(SceneView::with_convention(
            self.readiness.camera.0,
            self.readiness.camera.1,
            Viewport {
                width: self.size[0],
                height: self.size[1],
            },
            self.readiness.camera.2,
        )?))
    }

    /// Runs once on the main thread through the same event loop and renderer as `run_with_updates`.
    pub fn run(self, driver: &mut impl RetainedDriver) -> Result<(), ViewerError> {
        self.run_with_activity(
            driver,
            crate::activity::ActivityGuard::begin,
            crate::app::run_retained,
        )
    }

    pub(crate) fn run_with_activity<G>(
        self,
        driver: &mut dyn RetainedDriver,
        begin: impl FnOnce(&str) -> G,
        run: impl FnOnce(Self, &mut dyn RetainedDriver) -> Result<(), ViewerError>,
    ) -> Result<(), ViewerError> {
        let _activity = begin(self.options.activity_reason.as_ref());
        run(self, driver)
    }
}
