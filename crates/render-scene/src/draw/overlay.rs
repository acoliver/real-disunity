use super::SceneGpu;
use crate::{
    dynamic::{DynamicBindings, DynamicError, DynamicGpu, QuadTarget},
    simulation::UiOverlay,
};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) struct OverlayGpu {
    pub geometry: DynamicGpu,
    pub depth_test: bool,
    pub layout: wgpu::BindGroupLayout,
    source: Arc<UiOverlay>,
    /// The texture of the layer atlas the overlay's runs sample, when the host offers one.
    layers: Option<wgpu::Texture>,
}
impl SceneGpu {
    /// Replaces only native screen-space UI resources. Rejected uploads preserve residency. The
    /// layer atlas the overlay's runs show stays the one [`Self::bind_ui_layers`] bound.
    pub fn set_ui_overlay(
        &mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        overlay: &Arc<UiOverlay>,
    ) -> Result<(), DynamicError> {
        let layers = self.overlay.as_ref().and_then(|gpu| gpu.layers.clone());
        self.install_overlay(device, authority, overlay, layers)
    }

    /// Binds the host's layer atlas to the overlay's runs, or unbinds it with `None`, which
    /// leaves them transparent. Nothing is uploaded again while the texture is the one bound.
    pub fn bind_ui_layers(
        &mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        layers: Option<&wgpu::Texture>,
    ) -> Result<(), DynamicError> {
        let Some(current) = &self.overlay else {
            return Ok(());
        };
        if current.layers.as_ref() == layers {
            return Ok(());
        }
        let source = current.source.clone();
        self.install_overlay(device, authority, &source, layers.cloned())
    }

    fn install_overlay(
        &mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        overlay: &Arc<UiOverlay>,
        layers: Option<wgpu::Texture>,
    ) -> Result<(), DynamicError> {
        let layout = crate::upload::material_bind_group_layout(device);
        let mut geometry = DynamicGpu::default();
        geometry.set_textures(device, authority.queue(), &overlay.textures)?;
        if let Some((slot, atlas)) = overlay.layer.zip(layers.as_ref()) {
            geometry.replace_texture(slot, atlas.clone());
        }
        geometry.set_quads(
            device,
            authority.queue(),
            DynamicBindings {
                placeholder: &self.retained.placeholder,
                sampler: &self.retained.sampler,
                target: QuadTarget::Overlay(&layout),
            },
            &overlay.quads,
        )?;
        self.overlay = Some(Rc::new(OverlayGpu {
            geometry,
            layout,
            depth_test: overlay.quads.iter().any(|q| q.depth_test),
            source: overlay.clone(),
            layers,
        }));
        Ok(())
    }
}
