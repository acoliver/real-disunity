//! Observation of the selected adapter before GPU scene preparation.
use std::sync::Arc;

pub use worldrunner_render_scene::device::{AdapterClass, AdapterIdentity};

type Observe = dyn Fn(&AdapterIdentity) -> Result<(), String> + Send + Sync;

/// Called once at renderer startup, on its calling thread, before scene preparation.
/// The identity comes from the same adapter used to create the device, including
/// surface compatibility in desktop mode. No second adapter selection is performed.
/// Capture shared caller state to record a quality decision for later scene edits.
/// Returning an error stops startup before any scene allocation or upload.
#[derive(Clone)]
pub struct AdapterCallback(Arc<Observe>);

impl AdapterCallback {
    pub fn new(
        observe: impl Fn(&AdapterIdentity) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(observe))
    }

    pub(crate) fn notify(&self, adapter: &wgpu::Adapter) -> Result<(), crate::ViewerError> {
        (self.0)(&adapter.get_info().into()).map_err(crate::ViewerError::Driver)
    }
}

impl std::fmt::Debug for AdapterCallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AdapterCallback")
    }
}
