//! Exact canonical translations qualified against one native/extractor/GPU generation.
//! This module grants no original Unity shader, artwork, font or gameplay fidelity.
mod binding;
mod catalog;
mod gpu;
mod native;
mod qualify;
mod stage;

use binding::Binding;
use catalog::Provider;
use std::path::Path;
use worldrunner::{
    compat_content::{SelectedResource, TrustedProviderInventory},
    managed::CompatibilityHost,
    scene::gpu::device::GpuContext,
};

/// Host-owned services and immutable exact resources. Authority cannot be deserialized.
/// The retained world is a zero-frame native qualification fixture, never a game world.
pub struct ResourceProviders<'gpu> {
    host: CompatibilityHost,
    world: u64,
    binding: Binding,
    assets: crate::extraction::AssetTable,
    renderer: gpu::Renderer<'gpu>,
    providers: Vec<Provider>,
    evidence: Vec<u8>,
}
impl<'gpu> ResourceProviders<'gpu> {
    /// Execute fresh native, extraction and canonical GPU contracts on the caller's device.
    /// Equality witnesses cover selected source inputs, not all transitive Rust definitions.
    pub fn qualify(root: &Path, context: &'gpu GpuContext) -> Result<Self, String> {
        qualify::run(root, context, gpu::Control::Connected)
    }
    /// Atomically register all exact bindings after rechecking this live generation.
    pub fn register(&self, inventory: &mut TrustedProviderInventory) -> Result<(), String> {
        self.binding.verify()?;
        self.binding.verify_mapped()?;
        self.host
            .request(self.world, &serde_json::json!({"op":"snapshot"}))
            .map_err(|e| e.to_string())?;
        catalog::verify(&self.providers, &self.binding, &self.evidence)?;
        if !inventory.bindings().is_empty() {
            return Err(
                "resource services require a fresh inventory for this separate generation".into(),
            );
        }
        let mut candidate = inventory.clone();
        for provider in &self.providers {
            candidate.register(provider)?;
        }
        *inventory = candidate;
        Ok(())
    }
    /// Exact selectors for this generation; unsupported resources have no descriptor.
    pub fn selected_resources(&self) -> Vec<SelectedResource> {
        catalog::selected()
    }
    pub fn evidence_bytes(&self) -> &[u8] {
        &self.evidence
    }
    /// Render the retained native fixture through the same owned canonical services.
    /// This is a resource service witness. It does not load a translated game graph.
    pub fn capture(&mut self, material_id: &str) -> Result<Vec<u8>, String> {
        self.binding.verify()?;
        qualify::capture(
            &self.host,
            self.world,
            &self.assets,
            &mut self.renderer,
            material_id,
        )
    }
}
impl Drop for ResourceProviders<'_> {
    fn drop(&mut self) {
        let _ = self.host.destroy_world(self.world);
    }
}

#[cfg(test)]
#[path = "resource_providers/tests.rs"]
mod tests;
