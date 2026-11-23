//! Stock paired depth. Relief reuse requires the shared single-sample receiver producer.
use super::FrameRenderer;
use crate::{
    draw::SceneGpu,
    renderer::depth::DepthMode,
    scene_metadata::{Geometry, SceneMetadata},
};
use worldrunner_render_plan::{SceneSsao, SceneSsr};
use worldrunner_render_post::{AttachmentView, FrameConfig, FrameError, SceneDepth};

/// The prepass metadata a stock frame owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Prepass {
    /// Occlusion computed with these settings every frame: SSAO receivers sample it, and
    /// enabled reflections read it as application occlusion (neutral settings without SSAO).
    Occlusion(SceneSsao),
    /// Only shadow depth fitting reads the prepass. The frame runs the prepass and level 0 of
    /// the depth hierarchy, receivers bind the constant neutral occlusion, and further stages
    /// run only when another feature of the frame declares a read of them.
    DepthOnly,
}

impl Prepass {
    pub(super) fn metadata(
        self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
    ) -> Result<SceneMetadata, FrameError> {
        match self {
            Self::Occlusion(settings) => SceneMetadata::new(device, size, settings),
            Self::DepthOnly => SceneMetadata::depth_only(device, queue, size),
        }
    }
}

/// The metadata a stock frame owns: the requested SSAO, otherwise neutral occlusion when
/// enabled reflections draw the depth and normal prepass, otherwise the depth alone when shadow
/// depth fitting does. The owner decides scene depth ownership, so Opaque reuses whichever
/// consumer's prepass depth. None draws no prepass.
pub(super) fn prepass_metadata(
    ssao: Option<SceneSsao>,
    ssr: Option<&SceneSsr>,
    depth_fitting: bool,
) -> Option<Prepass> {
    if let Some(settings) = ssao {
        Some(Prepass::Occlusion(settings))
    } else if ssr.is_some_and(|ssr| ssr.enabled) {
        Some(Prepass::Occlusion(crate::ssr::neutral_ao()))
    } else {
        depth_fitting.then_some(Prepass::DepthOnly)
    }
}

pub(super) fn allocation(
    features: wgpu::Features,
    config: FrameConfig,
    complete_prepass: bool,
) -> SceneDepth {
    if config.samples == 1
        && complete_prepass
        && features.contains(wgpu::Features::DEPTH32FLOAT_STENCIL8)
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
        if gpu.has_relief()
            && self
                .ssao
                .as_ref()
                .is_none_or(|screen| screen.receiver.is_none())
        {
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
        #[cfg(test)]
        if let Some(reference) = self.receiver_reference {
            return reference.depth();
        }
        if !depth.format().has_stencil_aspect() {
            return DepthMode::Independent;
        }
        let paired = self
            .ssao
            .as_ref()
            .is_some_and(|screen| screen.depth == *depth.texture());
        if paired {
            let local_receivers = self
                .ssao
                .as_ref()
                .is_none_or(|screen| screen.receiver.is_none());
            mode(
                depth.texture().sample_count(),
                true,
                gpu.has_relief() && local_receivers,
            )
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
            worldrunner_render_post::SceneDepth::Independent,
        )
        .unwrap();
        self.ssao = self
            .ssao
            .as_ref()
            .map(|screen| screen.rebuilt(device, config.size).unwrap());
        self.install_reflections(device).unwrap();
        if self.water.is_some() {
            self.register_water(device).unwrap();
        }
    }

    /// Replaces depth-only metadata with metadata that computes the neutral occlusion and the
    /// whole depth hierarchy, as depth-fitting-only frames did before they owned the depth alone:
    /// the reference those frames must reproduce byte for byte.
    #[cfg(test)]
    pub(crate) fn computed_neutral_occlusion_for_test(&mut self, device: &wgpu::Device) {
        let screen = self.ssao.as_ref().expect("prepass metadata");
        assert!(!screen.passes.computes_occlusion());
        self.ssao =
            Some(SceneMetadata::new(device, screen.size, crate::ssr::neutral_ao()).unwrap());
        self.share_metadata_depth(device);
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

    /// Shadow depth fitting draws the prepass even when no screen effect asks for it, so the
    /// frame must own that prepass and pair its depth with Opaque, as SSAO and SSR frames do.
    /// Nothing samples occlusion in that frame, so it owns the depth alone, while enabled
    /// reflections keep the computed neutral occlusion their application AO reads.
    #[test]
    fn depth_fitting_alone_owns_the_prepass_and_pairs_its_depth_with_opaque() {
        let features = wgpu::Features::DEPTH32FLOAT_STENCIL8;
        let single = FrameConfig::new([65, 49], wgpu::TextureFormat::Rgba16Float, 1);
        let disabled = SceneSsr {
            enabled: false,
            ..Default::default()
        };
        let fitting = prepass_metadata(None, Some(&disabled), true);
        assert_eq!(fitting, Some(Prepass::DepthOnly));
        assert_eq!(prepass_metadata(None, None, true), fitting);
        assert_eq!(
            allocation(features, single, fitting.is_some()),
            SceneDepth::Ownership
        );
        let neutral = Some(Prepass::Occlusion(crate::ssr::neutral_ao()));
        assert_eq!(
            prepass_metadata(None, Some(&SceneSsr::default()), false),
            neutral
        );
        assert_eq!(
            prepass_metadata(None, Some(&SceneSsr::default()), true),
            neutral
        );
        let requested = SceneSsao {
            sample_count: 16,
            ..crate::ssr::neutral_ao()
        };
        assert_eq!(
            prepass_metadata(Some(requested), None, true),
            Some(Prepass::Occlusion(requested))
        );
        assert_eq!(prepass_metadata(None, Some(&disabled), false), None);
        assert_eq!(allocation(features, single, false), SceneDepth::Independent);
        let multisampled = FrameConfig::new([65, 49], wgpu::TextureFormat::Rgba16Float, 4);
        assert_eq!(
            allocation(features, multisampled, fitting.is_some()),
            SceneDepth::Independent
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
