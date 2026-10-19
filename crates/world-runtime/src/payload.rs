//! Immutable type-erased payloads produced by exact runtime handlers.

use std::any::Any;
use std::fmt::{self, Debug};
use std::sync::Arc;

/// A renderer-ready payload produced by a component or capability handler.
///
/// Implementations are immutable once wrapped in [`PreparedPayload`]. The
/// payload contract URI identifies the external contract; this runtime type is an
/// additional local safety check for the consumer implementing that contract.
pub trait RuntimePayload: Any + Debug + Send + Sync {
    /// Exposes the concrete type for checked downcasting.
    fn as_any(&self) -> &dyn Any;
}

impl<T> RuntimePayload for T
where
    T: Any + Debug + Send + Sync,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Shared immutable ownership of one handler-produced typed payload.
#[derive(Clone)]
pub struct PreparedPayload {
    inner: Arc<dyn RuntimePayload>,
}

impl PreparedPayload {
    /// Wraps one concrete renderer-ready payload.
    pub fn new<T>(payload: T) -> Self
    where
        T: RuntimePayload,
    {
        Self {
            inner: Arc::new(payload),
        }
    }

    /// Returns the payload as `T` when the registered handler produced that
    /// exact Rust contract type.
    #[must_use]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.inner.as_ref().as_any().downcast_ref()
    }

    /// Returns whether this payload carries exactly `T`.
    #[must_use]
    pub fn is<T: Any>(&self) -> bool {
        self.inner.as_ref().as_any().is::<T>()
    }
}

impl Debug for PreparedPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedPayload")
            .field("type_id", &self.inner.as_ref().as_any().type_id())
            .field("value", &self.inner)
            .finish()
    }
}
