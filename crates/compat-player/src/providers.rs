//! Host-owned, behavior-qualified providers for the bounded keyboard/unlit profile.
//! Qualification data is compiled into trusted composition. Fixture JSON is never an authority.
#[path = "providers/binding.rs"]
pub(crate) mod binding;
#[path = "providers/compiled_inputs.rs"]
mod compiled_inputs;
#[path = "providers/input.rs"]
mod input;
#[path = "providers/render.rs"]
pub(crate) mod render;
#[path = "providers/source.rs"]
mod source;
pub use source::GameSource;

use std::path::Path;
use worldrunner::{compat_content::TrustedProviderInventory, managed::CompatibilityHost};

pub use binding::QualifiedProviders;
#[derive(Clone, Copy)]
pub(crate) enum RenderControl {
    Connected,
    #[cfg(test)]
    Disconnected,
    #[cfg(test)]
    WrongStage,
}

/// Boot the sealed implementation and execute its actual native/managed/GPU contract.
/// New images require a new independently retained qualification and trusted build.
pub fn qualify(
    root: &Path,
    context: &worldrunner::scene::gpu::device::GpuContext,
) -> Result<QualifiedProviders, String> {
    qualify_control(root, context, RenderControl::Connected)
}
fn qualify_control(
    root: &Path,
    context: &worldrunner::scene::gpu::device::GpuContext,
    control: RenderControl,
) -> Result<QualifiedProviders, String> {
    let binding = binding::Binding::load(root)?;
    let mut host = CompatibilityHost::boot(&binding.config()).map_err(|e| e.to_string())?;
    binding.verify_loaded_native()?;
    let input = input::qualify(&mut host, &binding)?;
    let render = render::qualify(&mut host, &binding, context, control)?;
    binding.verify()?;
    Ok(QualifiedProviders::new(host, binding, &input, &render))
}

impl QualifiedProviders {
    /// Register only this live composition after rechecking every sealed input.
    pub fn register(&self, inventory: &mut TrustedProviderInventory) -> Result<(), String> {
        self.verify_current()?;
        for provider in self.providers() {
            if inventory.bindings().contains_key(provider.id()) {
                return Err("qualified provider already registered".into());
            }
        }
        for provider in self.providers() {
            inventory.register(provider)?;
        }
        Ok(())
    }
}
