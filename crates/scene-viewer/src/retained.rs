//! CPU scene ownership and ordered edits for the shared native viewer.
use crate::{ViewerError, ViewerOptions};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use worldrunner_render_plan::{RenderScene, SceneCamera, SceneEffects, SceneObject};
use worldrunner_render_scene::{CameraPose, SceneView, Viewport};

mod session;
pub(crate) use session::Session;
#[cfg(test)]
mod tests;

/// Caller-selected identity. Initial objects have IDs 0 through `object_count - 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId(pub u64);

/// Ordered edits reference the initial scene's retained mesh and material tables.
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
    /// Lighting and post settings. Resource-changing SSAO, SSR, water, atmosphere,
    /// celestial disc/mesh and dome texture edits are rejected until asynchronous
    /// preparation is available.
    Settings(Box<SceneEffects>),
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
/// together on the next admitted frame; skipped acquisitions retain pending edits.
pub struct RetainedHost {
    pub(crate) scene: RenderScene,
    pub(crate) options: ViewerOptions,
    pub(crate) pending: Vec<RetainedUpdate>,
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
            scene,
            options,
            pending: Vec::new(),
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
        match &update {
            RetainedUpdate::Add { id, object } => {
                self.validate_object(*object)?;
                if self.objects.contains_key(id) {
                    return Err(ViewerError::Driver("duplicate object ID".into()));
                }
                self.objects.insert(*id, *object);
            }
            RetainedUpdate::Remove(id) => {
                self.objects
                    .remove(id)
                    .ok_or_else(|| ViewerError::Driver("unknown object ID".into()))?;
            }
            RetainedUpdate::Replace { id, object } => {
                self.validate_object(*object)?;
                let entry = self
                    .objects
                    .get_mut(id)
                    .ok_or_else(|| ViewerError::Driver("unknown object ID".into()))?;
                *entry = *object;
            }
            RetainedUpdate::Camera { pose, lens } => {
                SceneView::new(
                    *pose,
                    *lens,
                    Viewport {
                        width: 1,
                        height: 1,
                    },
                )?;
                self.options.pose = *pose;
                self.options.camera = *lens;
            }
            RetainedUpdate::Settings(effects) => self.validate_settings(effects)?,
        }
        self.pending.push(update);
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

    fn validate_settings(&self, effects: &SceneEffects) -> Result<(), ViewerError> {
        let Some(initial) = self.scene.effects() else {
            return Err(ViewerError::UnsupportedScene(
                "adding effects to an unlit scene",
            ));
        };
        if initial.ssao() != effects.ssao()
            || initial.ssr() != effects.ssr()
            || initial.water_bodies() != effects.water_bodies()
            || initial.atmosphere() != effects.atmosphere()
            || (initial.sky() != effects.sky()
                && matches!(
                    (initial.sky(), effects.sky()),
                    (worldrunner_render_plan::SceneSky::DomeTexture { .. }, _)
                        | (_, worldrunner_render_plan::SceneSky::DomeTexture { .. })
                ))
        {
            return Err(ViewerError::UnsupportedScene(
                "resource-changing retained settings",
            ));
        }
        worldrunner_render_scene::post_settings(effects)?;
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
            self.options.pose,
            self.options.camera,
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
