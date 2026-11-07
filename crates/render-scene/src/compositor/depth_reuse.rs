//! Stock frame planning for paired depth. Relief depth remains unproved across programs.
use super::FrameRenderer;
use crate::{draw::SceneGpu, renderer::depth::DepthMode, scene_metadata::Geometry};
use worldrunner_render_post::{AttachmentView, FrameConfig, SceneDepth};

pub(super) fn allocation(
    device: &wgpu::Device,
    config: FrameConfig,
    complete_prepass: bool,
) -> SceneDepth {
    if config.samples == 1
        && complete_prepass
        && device
            .features()
            .contains(wgpu::Features::DEPTH32FLOAT_STENCIL8)
    {
        SceneDepth::Ownership
    } else {
        SceneDepth::Independent
    }
}

pub(crate) fn mode(samples: u32, paired: bool, relief: bool) -> DepthMode {
    match (samples, paired, relief) {
        (1, true, false) => DepthMode::Reuse,
        (1, true, true) => DepthMode::Shared,
        _ => DepthMode::Independent,
    }
}

impl FrameRenderer {
    pub(crate) fn share_metadata_depth(&mut self, device: &wgpu::Device) {
        if self.targets().scene_depth() == SceneDepth::Ownership {
            let depth = self.targets().depth().texture().clone();
            if let Some(screen) = &mut self.ssao {
                screen.share_depth(device, &depth);
            }
        }
    }

    pub(crate) fn prepare_metadata_depth(&mut self, device: &wgpu::Device, gpu: &SceneGpu) {
        if self.targets().scene_depth() != SceneDepth::Ownership {
            return;
        }
        if gpu.has_relief() {
            if let Some(screen) = &mut self.ssao
                && screen.depth.format().has_stencil_aspect()
            {
                let depth = screen.independent_depth.clone();
                screen.share_depth(device, &depth);
            }
        } else {
            self.share_metadata_depth(device);
        }
    }

    pub(crate) fn forward_depth(&self, gpu: &SceneGpu, depth: &AttachmentView) -> DepthMode {
        if !depth.format().has_stencil_aspect() {
            return DepthMode::Independent;
        }
        let paired = self
            .ssao
            .as_ref()
            .is_some_and(|screen| screen.depth == *depth.texture());
        if paired {
            mode(depth.texture().sample_count(), true, gpu.has_relief())
        } else {
            DepthMode::Shared
        }
    }

    pub(crate) fn color_pipelines(
        &self,
        base: &crate::renderer::ScenePipelines,
        depth: DepthMode,
    ) -> crate::renderer::ScenePipelines {
        self.ssao
            .as_ref()
            .and_then(|screen| screen.geometry.forward.as_ref())
            .map_or(base, |(pipelines, _)| pipelines)
            .for_depth(depth)
    }

    #[cfg(test)]
    pub(crate) fn independent_depth_for_test(&mut self, device: &wgpu::Device) {
        let config = self.targets().config();
        self.chain = worldrunner_render_post::PostChain::new(
            device,
            config,
            worldrunner_render_post::Resources::default(),
            self.chain.factory().clone(),
        )
        .unwrap();
        self.ssao = self.ssao.as_ref().map(|screen| {
            crate::scene_metadata::SceneMetadata::new(device, config.size, screen.settings).unwrap()
        });
        self.install_reflections(device, config.size).unwrap();
        if self.water.is_some() {
            self.chain
                .register(
                    device,
                    Box::new(crate::water::WaterRegistration {
                        state: self.water_state.clone(),
                        samples: config.samples,
                    }),
                )
                .unwrap();
        }
    }

    pub(crate) fn geometry_for_depth(&self, depth: DepthMode) -> Option<Geometry> {
        self.ssao
            .as_ref()
            .map(|screen| screen.geometry_for_depth(depth))
    }

    pub(crate) fn scene_depth_resource(
        &self,
        depth: &AttachmentView,
    ) -> worldrunner_render_post::ResourceRef {
        let external = self.ssao.as_ref().is_some_and(|screen| {
            screen.depth.format() == wgpu::TextureFormat::Depth32FloatStencil8
                && screen.depth != *depth.texture()
        });
        super::attachment_resource(
            if external {
                "external scene depth"
            } else {
                "scene depth"
            },
            depth,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combined_depth_fits_the_existing_two_float_depth_budget() {
        use worldrunner_render_gpu::configuration::AllocationSet;
        let limits = wgpu::Limits::default();
        let mut old = AllocationSet::default();
        old.texture(
            "depth",
            [65, 49, 1],
            wgpu::TextureFormat::Depth32Float,
            [1, 1],
            2,
        );
        let mut shared = AllocationSet::default();
        shared.texture(
            "depth ownership",
            [65, 49, 1],
            SceneDepth::Ownership.format(),
            [1, 1],
            1,
        );
        assert_eq!(
            old.validate(&limits).unwrap(),
            shared.validate(&limits).unwrap()
        );
    }

    #[test]
    fn reuse_requires_complete_paired_single_sample_geometry_without_unproved_relief() {
        assert_eq!(mode(1, true, false), DepthMode::Reuse);
        assert_eq!(mode(1, true, true), DepthMode::Shared);
        for samples in [1, 2, 4, 8] {
            assert_eq!(mode(samples, false, false), DepthMode::Independent);
            assert_eq!(mode(samples, false, true), DepthMode::Independent);
            if samples > 1 {
                assert_eq!(mode(samples, true, false), DepthMode::Independent);
            }
        }
    }
}
