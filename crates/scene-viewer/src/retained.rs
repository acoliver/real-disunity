//! CPU scene ownership and ordered edits for the shared native viewer.
use crate::{ViewerError, ViewerOptions};
use std::{collections::BTreeMap, time::Duration};
use worldrunner_render_plan::{RenderScene, SceneCamera, SceneEffects, SceneObject};
use worldrunner_render_scene::{CameraPose, SceneView, Viewport};

mod windowless;
pub use windowless::WindowlessRetained;
#[cfg(all(test, target_os = "linux"))]
pub(crate) use windowless::tests::pages::layered_scene;
mod input;
pub use input::{
    CursorError, CursorGrabMode, KeyCode, LogicalKey, MouseButton, NativeKeyCode, PhysicalKey,
    WheelDelta,
};
mod capture;
pub use capture::{
    CaptureJitter, CaptureOutput, CaptureRequestError, RetainedCaptureOutcome,
    RetainedCaptureProducts, RetainedCaptureRecipe,
};
mod readiness;
pub(crate) mod session;
pub(crate) use session::Session;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod terrain_tests;
/// Caller-selected identity. Initial objects have IDs 0 through `object_count - 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

/// Ordered edits reference the latest accepted mesh and material tables.
#[derive(Clone, Debug)]
pub enum RetainedUpdate {
    Add {
        id: ObjectId,
        object: SceneObject,
    },
    Remove(ObjectId),
    Replace {
        id: ObjectId,
        object: SceneObject,
    },
    Camera {
        pose: CameraPose,
        lens: SceneCamera,
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
}

/// Typed OS input, without application action mapping or GPU types.
/// Key names follow `LiveInputEvent`'s lowercase convention.
#[derive(Clone, Debug, PartialEq)]
pub enum RetainedInput {
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
    fn event(&mut self, _event: &RetainedEvent) -> Result<(), String> {
        Ok(())
    }
}

/// CPU state is available without starting a display or GPU. Edits become visible
/// in generation order on admitted frames. CPU preparation never waits on the event
/// thread; GPU allocation/upload uses the existing renderer at publication.
pub struct RetainedHost {
    pub(crate) captures: capture::Captures,
    pub(crate) scene: RenderScene,
    pub(crate) initial_scene: RenderScene,
    pub(crate) readiness: readiness::Readiness,
    pub(crate) options: ViewerOptions,
    objects: BTreeMap<ObjectId, SceneObject>,
    input_state: input::InputState,
    size: [u32; 2],
    exit: bool,
    occluded: bool,
}

impl RetainedHost {
    pub fn new(scene: RenderScene, options: ViewerOptions) -> Result<Self, ViewerError> {
        let scene = crate::reflection_settings::startup_scene(scene, &options)?;
        crate::validate(&scene, &options)?;
        let objects = scene
            .geometry()
            .objects()
            .iter()
            .copied()
            .enumerate()
            .map(|(index, object)| (ObjectId(index as u64), object))
            .collect();
        let size = options.size;
        Ok(Self {
            captures: capture::Captures::default(),
            initial_scene: scene.clone(),
            readiness: readiness::Readiness {
                queue: std::collections::VecDeque::new(),
                limit: 64,
                accepted: 0,
                ready: 0,
                published: 0,
                capture_features: capture::features(scene.effects()),
                pages_enabled: scene
                    .terrain()
                    .and_then(worldrunner_render_plan::SceneTerrain::page_policy)
                    .is_some(),
                page_wait: false,
                camera: (options.pose, options.camera),
                #[cfg(test)]
                preparation_hook: None,
            },
            scene,
            options,
            objects,
            input_state: input::InputState::default(),
            size,
            exit: false,
            occluded: false,
        })
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
        if self.preparations_in_flight() == self.readiness.limit {
            return Err(ViewerError::BackPressure {
                limit: self.readiness.limit,
            });
        }
        if let RetainedUpdate::Camera { pose, lens } = &update {
            Self::validate_camera(*pose, *lens)?;
            self.options.pose = *pose;
            self.options.camera = *lens;
            self.enqueue(update, false);
            return Ok(());
        }
        if let RetainedUpdate::Terrain(terrain) = &update {
            let scene = self
                .scene
                .with_terrain(terrain.clone())
                .map_err(|error| ViewerError::Driver(error.to_string()))?;
            crate::validate(&scene, &self.options)?;
            self.scene = scene;
            self.enqueue(update, true);
            return Ok(());
        }
        self.apply_scene_update(update)
    }

    fn apply_scene_update(&mut self, update: RetainedUpdate) -> Result<(), ViewerError> {
        let mut objects = self.objects.clone();
        let mut effects = self.scene.effects().cloned();
        let mut replacement = None;
        let rebuild = match &update {
            RetainedUpdate::Add { id, object } => {
                self.validate_object(*object)?;
                if objects.insert(*id, *object).is_some() {
                    return Err(ViewerError::Driver("duplicate object ID".into()));
                }
                false
            }
            RetainedUpdate::Remove(id) => {
                objects
                    .remove(id)
                    .ok_or_else(|| ViewerError::Driver("unknown object ID".into()))?;
                false
            }
            RetainedUpdate::Replace { id, object } => {
                self.validate_object(*object)?;
                *objects
                    .get_mut(id)
                    .ok_or_else(|| ViewerError::Driver("unknown object ID".into()))? = *object;
                false
            }
            RetainedUpdate::Camera { .. } | RetainedUpdate::Terrain(_) => {
                unreachable!("camera and terrain accepted above")
            }
            RetainedUpdate::Settings(next) => {
                self.validate_streamed_effects(next)?;
                let rebuild = readiness::resource_settings_changed(self.scene.effects(), next);
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
        };
        let scene = self.edited_scene(replacement, &objects, effects)?;
        self.scene = scene;
        self.objects = objects;
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
            self.scene
                .with_objects_and_effects(objects.values().copied().collect(), effects)
                .map_err(|error| ViewerError::Driver(error.to_string()))
        }
    }

    fn validate_camera(pose: CameraPose, lens: SceneCamera) -> Result<(), ViewerError> {
        SceneView::new(
            pose,
            lens,
            Viewport {
                width: 1,
                height: 1,
            },
        )?;
        Ok(())
    }

    fn validate_object(&self, object: SceneObject) -> Result<(), ViewerError> {
        object
            .transform()
            .validate()
            .map_err(worldrunner_render_scene::draw::InstanceError::from)?;
        if object.mesh().raw() as usize >= self.scene.geometry().meshes().len()
            || object.material().raw() as usize >= self.scene.resources().materials().len()
        {
            return Err(ViewerError::Driver(
                "object references a nonresident resource".into(),
            ));
        }
        Ok(())
    }

    /// Returns transitions to deliver in order. Focus loss first releases keys in
    /// physical-key order, then buttons in button order, before the focus event.
    /// Repeated focus loss never repeats a release.
    pub fn input(&mut self, input: RetainedInput) -> Vec<RetainedInput> {
        let mut events = self.input_state.accept(&input);
        match &input {
            RetainedInput::Resized(size) => self.size = *size,
            RetainedInput::Occluded(value) => self.occluded = *value,
            RetainedInput::CloseRequested => self.exit = true,
            _ => {}
        }
        events.push(input);
        events
    }

    pub fn view(&self) -> Result<Option<SceneView>, ViewerError> {
        if self.size.contains(&0) || self.occluded {
            return Ok(None);
        }
        Ok(Some(SceneView::new(
            self.readiness.camera.0,
            self.readiness.camera.1,
            Viewport {
                width: self.size[0],
                height: self.size[1],
            },
        )?))
    }

    /// Runs once on the main thread through the same event loop and renderer as `run_with_updates`.
    pub fn run(self, driver: &mut impl RetainedDriver) -> Result<(), ViewerError> {
        crate::app::run_retained(self, driver)
    }
}
