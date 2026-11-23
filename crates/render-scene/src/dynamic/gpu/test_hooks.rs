//! Test-only accessors of this module's types.

use super::*;

impl DynamicGpu {
    /// Weak references to what the staged draws lend the runtime, for tests of its lifetime.
    pub(crate) fn probes(&self) -> Vec<std::sync::Weak<dyn std::any::Any + Send + Sync>> {
        self.draws
            .iter()
            .map(|draw| {
                let owner: Arc<dyn std::any::Any + Send + Sync> = draw.resources.clone();
                Arc::downgrade(&owner)
            })
            .collect()
    }
}
