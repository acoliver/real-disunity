#[cfg(test)]
mod budget_tests;

#[cfg(test)]
mod capture_budget_tests;

use crate::ViewerError;
mod setup;

use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::FrameConfig;
use worldrunner_render_scene::Viewport;
use worldrunner_render_scene::compositor::{ConfiguredFrameRenderer, RendererConfiguration};

#[derive(Clone, Copy)]
pub(crate) struct PresentationPolicy {
    pub budget: crate::PresentationBudget,
    pub recipes: bool,
}

impl PresentationPolicy {
    pub(super) fn inventory(
        self,
        mut settings: RendererConfiguration,
        capture_size: Viewport,
        limits: &wgpu::Limits,
    ) -> Result<RendererConfiguration, ViewerError> {
        settings.additional = capture_allocations(capture_size);
        if self.recipes {
            settings.additional.extend(recipe_allocations());
        }
        settings
            .allocations()
            .validate_budget::<worldrunner_render_post::PostError>(
                limits,
                AllocationBudget(self.budget.0),
            )?;
        Ok(settings)
    }
}

/// Keep the scene renderer's sRGB contract; copy output adds no new display look.
pub(super) const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub(super) struct Presentation {
    pub scheduler: ConfiguredFrameRenderer,
    pub targets: Targets,
    scale: Option<worldrunner_render_post::RenderScale>,
    capture_size: Viewport,
    recipe_capacity: bool,
    native_rcas: Option<crate::NativeRcasSettings>,
}

pub(super) struct Targets {
    pub color: wgpu::TextureView,
    pub depth: wgpu::TextureView,
}

impl Presentation {
    #[cfg(test)]
    pub fn new(
        context: &worldrunner_render_scene::device::GpuContext,
        format: wgpu::TextureFormat,
        size: Viewport,
    ) -> Self {
        Self::with_profile(
            context,
            format,
            size,
            size,
            crate::FrameProfile::Legacy,
            None,
        )
        .unwrap()
    }

    #[cfg(test)]
    pub fn with_profile(
        context: &worldrunner_render_scene::device::GpuContext,
        format: wgpu::TextureFormat,
        size: Viewport,
        capture_size: Viewport,
        profile: crate::FrameProfile,
        effects: Option<&worldrunner_render_plan::SceneEffects>,
    ) -> Result<Self, ViewerError> {
        Self::with_settings(
            context,
            format,
            [size, capture_size],
            profile,
            effects,
            PresentationPolicy {
                budget: crate::PresentationBudget::default(),
                recipes: true,
            },
        )
    }

    pub fn with_settings(
        context: &worldrunner_render_scene::device::GpuContext,
        format: wgpu::TextureFormat,
        [size, capture_size]: [Viewport; 2],
        profile: crate::FrameProfile,
        effects: Option<&worldrunner_render_plan::SceneEffects>,
        policy: PresentationPolicy,
    ) -> Result<Self, ViewerError> {
        let (settings, scale) =
            setup::configuration(context.device().features(), format, size, profile, effects)?;
        let settings = policy.inventory(settings, capture_size, &context.device().limits())?;
        let recipe_capacity = policy.recipes;
        let native_rcas = settings.post.native_rcas;
        let scheduler =
            ConfiguredFrameRenderer::new(context, settings, AllocationBudget(policy.budget.0))?;
        let targets = Targets::new(context.device(), &scheduler, size);
        Ok(Self {
            scheduler,
            targets,
            scale,
            capture_size,
            recipe_capacity,
            native_rcas,
        })
    }

    pub fn resize(
        &mut self,
        context: &worldrunner_render_scene::device::GpuContext,
        size: Viewport,
        capture_size: Viewport,
    ) -> Result<(), ViewerError> {
        self.resize_scaled(context, size, capture_size, self.scale, self.native_rcas)
    }

    pub fn resize_recipe(
        &mut self,
        context: &worldrunner_render_scene::device::GpuContext,
        size: Viewport,
        rcas: Option<crate::NativeRcasSettings>,
    ) -> Result<(), ViewerError> {
        self.resize_scaled(context, size, size, None, rcas.or(self.native_rcas))
    }

    fn resize_scaled(
        &mut self,
        context: &worldrunner_render_scene::device::GpuContext,
        size: Viewport,
        capture_size: Viewport,
        scale: Option<worldrunner_render_post::RenderScale>,
        rcas: Option<crate::NativeRcasSettings>,
    ) -> Result<(), ViewerError> {
        self.scheduler.collect();
        let extent = [size.width, size.height];
        let render = scale.map_or(Ok(extent), |scale| scale.extent(extent))?;
        if self.scheduler.configuration().frame.output_extent() != extent
            || self.scheduler.configuration().frame.size != render
            || self.capture_size != capture_size
            || self.scheduler.configuration().post.native_rcas != rcas
        {
            let mut settings = self.scheduler.configuration().clone();
            settings.post.native_rcas = rcas;
            settings.frame.size = [size.width, size.height];
            settings.frame.output_size = None;
            if let Some(scale) = scale {
                settings.frame = settings
                    .frame
                    .with_render_extent(scale.extent([size.width, size.height])?);
            }
            settings.additional = capture_allocations(capture_size);
            if self.recipe_capacity {
                settings.additional.extend(recipe_allocations());
            }
            self.scheduler.reconfigure(context, settings)?;
            self.targets = Targets::new(context.device(), &self.scheduler, size);
            self.capture_size = capture_size;
        }
        Ok(())
    }

    pub fn present(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        surface: &wgpu::TextureView,
    ) -> Result<(), ViewerError> {
        self.scheduler.encode_output(encoder, surface)?;
        Ok(())
    }

    pub fn refresh_targets(&mut self) {
        self.targets = Targets {
            color: self.scheduler.targets().scene().clone(),
            depth: self.scheduler.targets().depth().clone(),
        };
    }
}

fn recipe_allocations() -> worldrunner_render_gpu::configuration::AllocationSet {
    let mut allocations = worldrunner_render_gpu::configuration::AllocationSet::default();
    allocations.shared_cpu("capture recipe storage reservation", 256 * 1024 * 1024);
    allocations
}

fn capture_allocations(size: Viewport) -> worldrunner_render_gpu::configuration::AllocationSet {
    let mut allocations = worldrunner_render_gpu::configuration::AllocationSet::default();
    let extent = [size.width, size.height];
    allocations.readback("capture staging slots", extent, 4, 3);
    let padded = (u64::from(size.width) * 4).next_multiple_of(256) * u64::from(size.height);
    // Collection holds earlier completed images while unpadding the next mapped read.
    allocations.cpu("capture padded read", padded);
    allocations.cpu(
        "capture completed images",
        u64::from(size.width) * u64::from(size.height) * 4 * 3,
    );
    allocations
}

impl Targets {
    fn new(_device: &wgpu::Device, chain: &ConfiguredFrameRenderer, _size: Viewport) -> Self {
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
    pub fn for_device(size: [u32; 2], device: &wgpu::Device) -> Result<Option<Self>, ViewerError> {
        Self::new(size, device.limits().max_texture_dimension_2d)
    }

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
