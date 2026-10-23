use crate::ViewerError;
use worldrunner_render_post::FrameConfig;
use worldrunner_render_scene::Viewport;
use worldrunner_render_scene::compositor::FrameRenderer;

/// Keep the scene renderer's sRGB contract; copy output adds no new display look.
pub(super) const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub(super) struct Presentation {
    pub scheduler: FrameRenderer,
    pub targets: Targets,
}

pub(super) struct Targets {
    pub color: wgpu::TextureView,
    pub depth: wgpu::TextureView,
}

impl Presentation {
    #[cfg(test)]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: Viewport) -> Self {
        let chain = FrameRenderer::with_output(
            device,
            FrameConfig::new([size.width, size.height], COLOR, 1),
            format,
        )
        .expect("validated viewer extent");
        let targets = Targets::new(device, &chain, size);
        Self {
            scheduler: chain,
            targets,
        }
    }

    pub fn with_profile(
        context: &worldrunner_render_scene::device::GpuContext,
        format: wgpu::TextureFormat,
        size: Viewport,
        profile: crate::FrameProfile,
        effects: Option<&worldrunner_render_plan::SceneEffects>,
    ) -> Result<Self, ViewerError> {
        let scheduler = match profile {
            crate::FrameProfile::Legacy => FrameRenderer::with_output(
                context.device(),
                FrameConfig::new([size.width, size.height], COLOR, 1),
                format,
            )?,
            crate::FrameProfile::Hdr { samples, tone_map } => FrameRenderer::hdr(
                context,
                [size.width, size.height],
                format,
                samples,
                tone_map,
                effects,
            )?,
        };
        let targets = Targets::new(context.device(), &scheduler, size);
        Ok(Self { scheduler, targets })
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: Viewport) {
        if self
            .scheduler
            .resize(device, [size.width, size.height])
            .expect("validated viewer extent")
        {
            self.targets = Targets::new(device, &self.scheduler, size);
        }
    }

    pub fn present(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface: &wgpu::TextureView,
    ) -> Result<(), ViewerError> {
        self.scheduler.encode_output(encoder, surface)?;
        Ok(())
    }
}

impl Targets {
    fn new(_device: &wgpu::Device, chain: &FrameRenderer, _size: Viewport) -> Self {
        Self {
            color: chain.targets().scene().clone(),
            depth: chain.targets().depth().clone(),
        }
    }
}

/// Native presentation and scene attachments have different size contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Extents {
    pub surface: Viewport,
    pub render: Viewport,
}

impl Extents {
    pub fn placeholder() -> Self {
        Self {
            surface: Viewport {
                width: 1,
                height: 1,
            },
            render: Viewport {
                width: 1,
                height: 1,
            },
        }
    }

    pub fn new(size: [u32; 2], device_limit: u32) -> Result<Option<Self>, ViewerError> {
        if size.contains(&0) {
            return Ok(None);
        }
        if size.iter().any(|&side| side > device_limit) {
            return Err(ViewerError::SurfaceExtent { size, device_limit });
        }
        Ok(Some(Self {
            surface: Viewport {
                width: size[0],
                height: size[1],
            },
            render: bounded_viewport(size, device_limit).expect("nonzero supported extent"),
        }))
    }
}

/// Uniformly downscale attachments, not the native surface. Round each side to
/// the nearest pixel; projection uses the physical aspect, so rounding cannot
/// change scene geometry's apparent aspect after the full-surface blit.
fn bounded_viewport(size: [u32; 2], device_limit: u32) -> Option<Viewport> {
    let longest = *size.iter().max()?;
    let limit = device_limit.min(4096);
    if size.contains(&0) || limit == 0 {
        return None;
    }
    let denominator = u64::from(longest.max(limit));
    let scale = |side| {
        u32::try_from((u64::from(side) * u64::from(limit) + denominator / 2) / denominator)
            .expect("scaled dimension is at most 4096")
            .max(1)
    };
    Some(Viewport {
        width: scale(size[0]),
        height: scale(size[1]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_extents_are_preserved_or_rejected_before_configuration() {
        for (physical, render) in [
            ([5120, 2880], [4096, 2304]),
            ([2880, 5120], [2304, 4096]),
            ([8192, 1], [4096, 1]),
            ([257, 241], [257, 241]),
        ] {
            let extents = Extents::new(physical, 8192).unwrap().unwrap();
            assert_eq!([extents.surface.width, extents.surface.height], physical);
            assert_eq!([extents.render.width, extents.render.height], render);
        }
        for physical in [[8193, 100], [100, 8193], [u32::MAX, u32::MAX]] {
            assert!(
                matches!(Extents::new(physical, 8192), Err(ViewerError::SurfaceExtent { size, device_limit: 8192 }) if size == physical)
            );
        }
        assert_eq!(Extents::new([0, u32::MAX], 8192).unwrap(), None);
        assert!(matches!(
            Extents::new([1, 1], 0),
            Err(ViewerError::SurfaceExtent { .. })
        ));
        assert_eq!(
            bounded_viewport([u32::MAX, 8000], 8192),
            Some(Viewport {
                width: 4096,
                height: 1
            })
        );
    }

    #[test]
    fn zero_area_suspends_and_oversized_extents_are_bounded() {
        assert_eq!(bounded_viewport([0, 100], 8192), None);
        assert_eq!(bounded_viewport([100, 0], 8192), None);
        assert_eq!(bounded_viewport([100, 100], 0), None);
        assert_eq!(
            bounded_viewport([5120, 2880], 8192),
            Some(Viewport {
                width: 4096,
                height: 2304
            })
        );
        assert_eq!(
            bounded_viewport([500, 900], 256),
            Some(Viewport {
                width: 142,
                height: 256
            })
        );
        assert_eq!(
            bounded_viewport([257, 241], 8192),
            Some(Viewport {
                width: 257,
                height: 241
            })
        );
    }
}

#[cfg(test)]
mod source_graph_tests {
    #[test]
    fn retired_presentation_shader_is_absent() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/presentation.wgsl");
        assert!(
            !path.exists(),
            "CopyOutput is the canonical viewer output shader"
        );
    }
}
