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

mod clock;
mod coroutines;
mod scheduled_contacts;
pub use scheduled_contacts::ContactId;
mod input;
mod ratio;
mod timeline;
pub use clock::Clock;
pub use coroutines::{CoroutineId, WaitCondition};
pub use input::{AxisBinding, AxisId, InputAction, InputEvent, InputPhase, InputView, KeyCode};
pub use ratio::{TimeError, TimeSpan};
pub use timeline::{
    Advance, CallbackOutcome, CallbackTicket, CallbackWork, FrameSummary, ScriptFailure,
};
mod components;
mod edits;
mod handles;
pub mod physics2d;
pub use edits::ObjectEdit;
mod clone;
mod fields;
mod hierarchy;
mod lifecycle;
mod lifetime;
mod transform;
mod world;
pub use clone::Instantiation;
pub use fields::FieldValue;
pub use hierarchy::ParentSpace;
pub use lifecycle::{LifecycleWork, MutationId, ScriptMessage};
pub use lifetime::DestroyWhen;

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
    #[error("invalid time: {0}")]
    InvalidTime(#[from] TimeError),
    #[error("input timestamp order or cutoff violation")]
    InputOrder,
    #[error("input capacity exceeded")]
    InputCapacity,
    #[error("invalid normalized input")]
    InvalidInput,
    #[error("advance already in progress")]
    AdvanceInProgress,
    #[error("wrong callback ticket or outcome")]
    WrongCallback,
    #[error("script is not ready")]
    NotReady,
    #[error("contact work is no longer current")]
    InvalidContact,
    #[error("callback failed: {0:?}")]
    CallbackFailed(ScriptFailure),
    #[error("world is faulted")]
    Faulted,
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
    #[error("parenting would create a hierarchy cycle")]
    HierarchyCycle,
    #[error("parent matrix is singular")]
    SingularParent,
    #[error("local matrix cannot be represented as TRS")]
    NonRepresentableLocalTransform,
    #[error("invalid or excessive script fields")]
    InvalidFields,
    #[error("component is not a script")]
    NotScript,
    #[error("lifecycle mutation does not belong to this world")]
    WrongMutation,
    #[error("physics: {0}")]
    Physics(#[from] physics2d::PhysicsError),
}
