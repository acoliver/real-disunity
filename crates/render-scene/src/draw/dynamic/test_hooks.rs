//! Test-only accessors of this module's types.

use super::*;

impl SceneGpu {
    pub(crate) fn dynamic_probes(&self) -> Vec<std::sync::Weak<dyn std::any::Any + Send + Sync>> {
        self.dynamic.probes()
    }
}
