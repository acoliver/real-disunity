use super::FrameRenderer;
use worldrunner_render_post::{
    BorrowedFeature, FeatureDeclaration, FeaturePass, FrameError, PassSlot,
};

impl FrameRenderer {
    pub(crate) fn capture_texture_keys(
        &mut self,
        textures: &[&wgpu::Texture],
    ) -> Result<Vec<worldrunner_render_gpu::submission::capture::WriteKey>, FrameError> {
        self.chain
            .capture_write_keys(textures)
            .map_err(FrameError::from)
    }
    pub(crate) fn clear_capture_resources(&mut self) {
        self.chain.clear_capture_resources();
    }

    pub(crate) fn encode_depth_prefix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        frame: &crate::frame::FrameUniform,
        source: &crate::scene_metadata::SceneMetadata,
    ) -> Result<(), FrameError> {
        let (gpu, pipelines) = scene;
        gpu.reject_streaming("streaming terrain prepasses")?;
        gpu.prepare_depth_prefix_pages(device, encoder, frame, source.size)?;
        let mut passes = Vec::new();
        let mut views = Vec::new();
        crate::ssao::passes::prepass(source, &mut passes, &mut views)?;
        let screen_declaration = source.passes.schedule_metadata(&mut passes, &mut views)?;
        let mut prepass = FeaturePass::new(PassSlot::DepthNormalPrepass);
        prepass.color_format = Some(wgpu::TextureFormat::Rgba16Float);
        prepass.depth_format = Some(wgpu::TextureFormat::Depth32Float);
        let mut geometry = super::SceneFeature {
            publications: crate::draw::DrawPublications::default(),
            gpu,
            pipelines,
            frame: *frame,
            screen: Some(source.geometry.clone()),
        };
        let mut screen = crate::ssao::passes::ScreenFeature {
            ssao: source,
            frame: *frame,
        };
        self.chain.encode_geometry_prefix(
            device,
            encoder,
            &worldrunner_render_post::FramePlan {
                passes: &passes,
                views: &views,
                requested: &[],
            },
            &mut [
                BorrowedFeature {
                    declaration: FeatureDeclaration {
                        passes: vec![prepass],
                        effects: vec![],
                    },
                    prepared: &mut geometry,
                },
                BorrowedFeature {
                    declaration: screen_declaration,
                    prepared: &mut screen,
                },
            ],
        )
    }

    pub(crate) fn capture_effect_keys(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<Vec<worldrunner_render_gpu::submission::capture::WriteKey>, FrameError> {
        if (self.water.is_some() || self.ssr.is_some()) && self.snapshots.is_none() {
            self.snapshots = Some(std::array::from_fn(|_| {
                crate::water::snapshots::Snapshots::new(device, self.targets().config())
            }));
        }
        let mut textures = Vec::new();
        if let Some(screen) = &self.ssao {
            textures.extend(screen.passes.textures());
            textures.extend([&screen.depth, &screen.normals]);
        }
        if let Some(water) = &self.water {
            textures.extend(water.receivers[self.frame_slot].iter());
        }
        if let Some(ssr) = &self.ssr {
            textures.extend(ssr.textures());
        }
        if let Some(snapshots) = &self.snapshots {
            let snapshot = &snapshots[self.frame_slot];
            textures.extend([&snapshot.color, &snapshot.depth, &snapshot.final_depth]);
        }
        self.chain
            .capture_write_keys(&textures)
            .map_err(FrameError::from)
    }

    pub fn capture_write_keys(&self) -> Vec<worldrunner_render_gpu::submission::capture::WriteKey> {
        vec![
            super::publication::key(&self.snapshot_ready),
            super::publication::key(&self.post_time),
            super::publication::key(&self.exposure_reset),
        ]
    }
}
