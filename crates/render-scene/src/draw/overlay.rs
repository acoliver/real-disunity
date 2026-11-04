use super::SceneGpu;
use crate::{
    dynamic::{DynamicBindings, DynamicError, DynamicGpu},
    simulation::UiOverlay,
};
use std::rc::Rc;

#[derive(Debug)]
pub(crate) struct OverlayGpu {
    pub geometry: DynamicGpu,
    pub layout: wgpu::BindGroupLayout,
}
impl SceneGpu {
    /// Replaces only native screen-space UI resources. Rejected uploads preserve residency.
    pub fn set_ui_overlay(
        &mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        overlay: &UiOverlay,
    ) -> Result<(), DynamicError> {
        let layout = crate::upload::material_bind_group_layout(device);
        let mut geometry = DynamicGpu::default();
        geometry.set_textures(device, authority.queue(), &overlay.textures)?;
        geometry.set_quads(
            device,
            authority.queue(),
            DynamicBindings {
                layout: &layout,
                placeholder: &self.retained.placeholder,
                sampler: &self.retained.sampler,
            },
            &overlay.quads,
        )?;
        self.overlay = Some(Rc::new(OverlayGpu { geometry, layout }));
        Ok(())
    }
}
