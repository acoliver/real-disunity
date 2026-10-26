//! CPU scene ownership and ordered edits for the shared native viewer.
use crate::{ViewerError, ViewerOptions};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use worldrunner_render_plan::{RenderScene, SceneCamera, SceneEffects, SceneObject};
use worldrunner_render_scene::{CameraPose, SceneView, Viewport};

mod capture;
pub use capture::{CaptureRequestError, RetainedCaptureOutcome};
mod readiness;
mod session;
pub(crate) use session::Session;
#[cfg(test)]
mod tests;

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
    /// Replace resource tables before adding or replacing objects that reference them.
    /// Existing objects and terrain must remain valid against the new tables.
    /// Textures requiring features absent from the initial device are rejected
    /// during publication.
    Resources {
        resources: worldrunner_render_plan::SceneResources,
        meshes: Vec<worldrunner_render_plan::SceneMesh>,
    },
}

/// Logical OS input, independent of window-library and GPU types.
/// Key names follow `LiveInputEvent`'s lowercase convention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetainedInput {
    Key {
        key: String,
        pressed: bool,
        repeat: bool,
    },
    Focused(bool),
    Occluded(bool),
    Resized([u32; 2]),
    SurfaceRecreated,
    CloseRequested,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

/// Application callbacks run on the native event thread. No GPU calls are needed.
pub trait RetainedDriver {
    fn update(&mut self, host: &mut RetainedHost, tick: &RetainedTick) -> Result<(), String>;
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
    held: BTreeSet<String>,
    size: [u32; 2],
    exit: bool,
    occluded: bool,
}

impl RetainedHost {
    pub fn new(scene: RenderScene, options: ViewerOptions) -> Result<Self, ViewerError> {
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
                camera: (options.pose, options.camera),
                #[cfg(test)]
                preparation_hook: None,
            },
            scene,
            options,
            objects,
            held: BTreeSet::new(),
            size,
            exit: false,
            occluded: false,
        })
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
    pub fn key_held(&self, key: &str) -> bool {
        self.held.contains(key)
    }
    pub fn request_exit(&mut self) {
        self.exit = true;
    }

    /// Invalid edits fail before changing CPU state or enqueuing GPU work.
    pub fn apply(&mut self, update: RetainedUpdate) -> Result<(), ViewerError> {
        if self.readiness.queue.len() == self.readiness.limit {
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
            RetainedUpdate::Camera { .. } => unreachable!("camera accepted above"),
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

    /// Returns transitions to deliver in order, including synthesized key releases
    /// before focus loss. Repeated focus loss never repeats a release.
    pub fn input(&mut self, input: RetainedInput) -> Vec<RetainedInput> {
        let mut events = Vec::new();
        match &input {
            RetainedInput::Key { key, pressed, .. } => {
                if *pressed {
                    self.held.insert(key.clone());
                } else {
                    self.held.remove(key);
                }
            }
            RetainedInput::Focused(false) => {
                for key in std::mem::take(&mut self.held) {
                    events.push(RetainedInput::Key {
                        key,
                        pressed: false,
                        repeat: false,
                    });
                }
            }
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
