//! Owned CPU render payloads and the adapters that mint them.

use std::any::Any;
use std::fmt::{self, Debug};
use std::sync::Arc;

use worldrunner_world_runtime::PreparedPayload;

use crate::error::RenderAdapterError;

/// A fully owned immutable CPU render payload produced by one adapter.
///
/// Implementations are arbitrary plain Rust values: they carry no paths, no
/// readers, no provider borrows, no memory maps, and no publication lifetimes.
/// The blanket implementation covers every type that is already `Any`,
/// `Debug`, `Send`, and `Sync`.
pub trait CpuRenderPayload: Any + Debug + Send + Sync {
    /// Exposes the concrete type for checked downcasting.
    fn as_any(&self) -> &dyn Any;
}

impl<T> CpuRenderPayload for T
where
    T: Any + Debug + Send + Sync,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Shared immutable ownership of one adapted CPU render payload.
///
/// The wrapper is cheap to clone and carries no lifetime of the publication,
/// contract, or planner that produced it, so a [`crate::RenderWorldPlan`] stays
/// valid after its inputs are dropped.
#[derive(Clone)]
pub struct RenderPayload {
    inner: Arc<dyn CpuRenderPayload>,
}

impl RenderPayload {
    /// Wraps one concrete owned CPU render payload.
    pub fn new<T>(payload: T) -> Self
    where
        T: CpuRenderPayload,
    {
        Self {
            inner: Arc::new(payload),
        }
    }

    /// Returns the payload as `T` when the adapter produced that exact type.
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

impl Debug for RenderPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RenderPayload")
            .field("type_id", &self.inner.as_ref().as_any().type_id())
            .field("value", &self.inner)
            .finish()
    }
}

/// Adapts one prepared runtime payload into a fully owned CPU render payload.
///
/// Implementations receive the immutable [`PreparedPayload`] produced by the
/// runtime handler and must return a [`RenderPayload`] whose contents are
/// entirely owned CPU state: anything borrowed from the publication, its
/// provider, or the filesystem fails this contract by construction.
pub trait RenderPayloadAdapter: Send + Sync {
    /// Adapts one prepared payload into owned CPU render state.
    ///
    /// # Errors
    ///
    /// Returns [`RenderAdapterError`] when the prepared payload carries the
    /// wrong Rust contract type or violates the adapter's content contract.
    fn adapt(&self, payload: &PreparedPayload) -> Result<RenderPayload, RenderAdapterError>;
}
