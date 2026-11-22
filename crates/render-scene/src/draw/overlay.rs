use super::SceneGpu;
use crate::{
    dynamic::{
        DynamicBindings, DynamicError, DynamicGpu, ExternalTexture, QuadTarget, create_sampler,
    },
    simulation::{UiOverlay, UiTarget},
    texture::prepare_texture,
    upload::upload_texture,
};
use std::rc::Rc;
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace, TextureSampler};

#[derive(Debug)]
pub(crate) struct OverlayGpu {
    pub geometry: DynamicGpu,
    pub depth_test: bool,
    pub layout: wgpu::BindGroupLayout,
}
impl SceneGpu {
    /// Replaces only native screen-space UI resources. Rejected uploads preserve residency.
    /// `targets` are the host's render textures the overlay's draws read, by the ids
    /// `overlay.targets()` lists; a slot that no target names, or whose texture the host has not
    /// made, draws as transparent black. Offering a different texture for an id means staging the
    /// overlay again.
    pub fn set_ui_overlay(
        &mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        overlay: &UiOverlay,
        targets: &[UiTarget],
    ) -> Result<(), DynamicError> {
        let layout = crate::upload::material_bind_group_layout(device);
        let mut geometry = DynamicGpu::default();
        geometry.set_textures(device, authority.queue(), &overlay.textures)?;
        geometry.set_external(external_textures(
            device,
            authority.queue(),
            overlay,
            targets,
        )?);
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
        }));
        Ok(())
    }
}

/// The texture of each slot of the overlay: the host's, or one transparent texel.
fn external_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    overlay: &UiOverlay,
    targets: &[UiTarget],
) -> Result<Vec<ExternalTexture>, DynamicError> {
    let mut unwritten = None;
    let mut external = Vec::with_capacity(overlay.targets.len());
    for id in &overlay.targets {
        let offered = targets
            .iter()
            .find(|target| target.id() == *id)
            .and_then(|target| Some((target.texture()?, target.sampler())));
        external.push(if let Some((texture, sampler)) = offered {
            ExternalTexture {
                texture: texture.clone(),
                sampler: create_sampler(device, sampler),
            }
        } else if let Some(held) = &unwritten {
            ExternalTexture::clone(held)
        } else {
            unwritten.insert(transparent_texel(device, queue)?).clone()
        });
    }
    Ok(external)
}

fn transparent_texel(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<ExternalTexture, DynamicError> {
    let texel = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0; 4].into(),
    };
    let prepared =
        prepare_texture(&texel).map_err(|source| DynamicError::Texture { index: 0, source })?;
    Ok(ExternalTexture {
        texture: upload_texture(device, queue, &prepared)?,
        sampler: create_sampler(device, TextureSampler::default()),
    })
}
