//! Typed native reasons survive until the ABI result is encoded.
use worldrunner_world_simulation::{ScriptFailure, WorldError};

#[derive(Debug)]
pub enum AbiError {
    Native(WorldError),
    Message(String),
    Aborted(ScriptFailure),
}
impl AbiError {
    pub fn reason(&self) -> crate::abi::DisunityErrorReason {
        use WorldError as W;
        match self {
            Self::Aborted(_) => crate::abi::DisunityErrorReason::AbortedScope,
            Self::Message(_) => crate::abi::DisunityErrorReason::InvalidArgument,
            Self::Native(error) => match error {
                W::InvalidTime(_) => crate::abi::DisunityErrorReason::InvalidTime,
                W::WrongCallback => crate::abi::DisunityErrorReason::WrongCallback,
                W::CallbackFailed(_) => crate::abi::DisunityErrorReason::CallbackFailed,
                W::Faulted => crate::abi::DisunityErrorReason::Faulted,
                W::CapacityExceeded(_) | W::InputCapacity => {
                    crate::abi::DisunityErrorReason::Capacity
                }
                W::SingularParent => crate::abi::DisunityErrorReason::SingularParent,
                W::Physics(_) => crate::abi::DisunityErrorReason::Physics,
                W::StaleObject(_) | W::StaleComponent(_) | W::WrongWorld => {
                    crate::abi::DisunityErrorReason::StaleHandle
                }
                W::AdvanceInProgress => crate::abi::DisunityErrorReason::AdvanceInProgress,
                W::InvalidComponent(_) => crate::abi::DisunityErrorReason::InvalidComponent,
                W::Ui(_) => crate::abi::DisunityErrorReason::Ui,
                W::NotReady => crate::abi::DisunityErrorReason::NotReady,
                W::InvalidObject
                | W::HierarchyCycle
                | W::NonRepresentableLocalTransform
                | W::InvalidFields
                | W::NotScript
                | W::WrongMutation
                | W::InputOrder
                | W::InvalidInput
                | W::InvalidContact
                | W::SpatialIndexDisabled
                | W::InvalidSpatialArgument => crate::abi::DisunityErrorReason::InvalidArgument,
            },
        }
    }
}
impl std::fmt::Display for AbiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Native(WorldError::InvalidTime(error)) => error.fmt(f),
            Self::Native(error) => error.fmt(f),
            Self::Message(message) => message.fmt(f),
            Self::Aborted(failure) => write!(f, "aborted callback scope: {}", failure.message),
        }
    }
}
impl std::error::Error for AbiError {}
impl From<WorldError> for AbiError {
    fn from(value: WorldError) -> Self {
        Self::Native(value)
    }
}
impl From<worldrunner_world_simulation::TimeError> for AbiError {
    fn from(value: worldrunner_world_simulation::TimeError) -> Self {
        Self::Native(WorldError::InvalidTime(value))
    }
}
impl From<worldrunner_world_simulation::UiError> for AbiError {
    fn from(value: worldrunner_world_simulation::UiError) -> Self {
        Self::Native(WorldError::Ui(value))
    }
}
impl From<&str> for AbiError {
    fn from(value: &str) -> Self {
        Self::Message(value.into())
    }
}
impl From<String> for AbiError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<crate::store::AllocationError> for AbiError {
    fn from(value: crate::store::AllocationError) -> Self {
        use crate::store::AllocationError as A;
        match value {
            A::InvalidWorld => WorldError::WrongWorld.into(),
            A::HandleCapacity => {
                WorldError::CapacityExceeded(worldrunner_world_simulation::LimitKind::Generation)
                    .into()
            }
            A::Native(error) => error.into(),
        }
    }
}
