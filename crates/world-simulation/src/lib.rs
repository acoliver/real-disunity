//! CPU-only live world storage, independent of foreign runtimes and serialization.
//!
//! ```
//! use worldrunner_world_simulation::{ObjectDesc, World, WorldSettings};
//! let mut world = World::new(WorldSettings::default())?;
//! let object = world.create_object(ObjectDesc::default())?;
//! assert_eq!(world.snapshot().objects.len(), 1);
//! world.destroy_object(object)?;
//! # Ok::<(), worldrunner_world_simulation::WorldError>(())
//! ```

mod components;
mod edits;
mod handles;
pub use edits::ObjectEdit;
mod world;

pub use components::{ComponentDesc, LocalTransform};
pub use handles::{ComponentHandle, ObjectHandle};
pub use world::{
    ComponentSnapshot, ObjectDesc, ObjectSnapshot, World, WorldSettings, WorldSnapshot,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitKind {
    Objects,
    Components,
    Generation,
    WorldIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ComponentError {
    #[error("duplicate singleton component")]
    Duplicate,
    #[error("the object's transform cannot be removed")]
    RequiredTransform,
    #[error("invalid transform: expected finite TRS and unit quaternion")]
    InvalidTransform,
    #[error("script assembly and type must contain 1..1024 bytes")]
    InvalidScript,
    #[error("component kind cannot be changed")]
    KindMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorldError {
    #[error("handle belongs to another world")]
    WrongWorld,
    #[error("stale object handle: {0:?}")]
    StaleObject(ObjectHandle),
    #[error("stale component handle: {0:?}")]
    StaleComponent(ComponentHandle),
    #[error("invalid component: {0}")]
    InvalidComponent(ComponentError),
    #[error("capacity exceeded: {0:?}")]
    CapacityExceeded(LimitKind),
    #[error("object name/tag must be at most 1024 bytes and layer in 0..31")]
    InvalidObject,
}
