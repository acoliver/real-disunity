use super::FrameRenderer;
use worldrunner_render_post::{
    BorrowedFeature, FeatureDeclaration, FeaturePass, FrameError, PassSlot,
};

fn prefix_depth(
    gpu: &crate::draw::SceneGpu,
    source: &crate::scene_metadata::SceneMetadata,
) -> crate::renderer::depth::DepthMode {
    use crate::renderer::depth::DepthMode;
    if !source.depth.format().has_stencil_aspect() {
        DepthMode::Independent
    } else if gpu.has_relief() && source.receiver.is_none() {
        DepthMode::Shared
    } else {
        DepthMode::Reuse
    }
}

impl FrameRenderer {
    pub(crate) fn draw_publications(&self) -> crate::draw::DrawPublications {
        crate::draw::DrawPublications::with_producer(self.producer_draw.clone())
    }

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
        gpu.prepare_dynamic(encoder, pipelines)?;
        gpu.prepare_terrain_geometry(device, encoder, frame, source.size)?;
        gpu.prepare_depth_prefix_pages(device, encoder, frame, source.size)?;
        let mut passes = Vec::new();
        let mut views = Vec::new();
        let material = self.ssr.as_ref().map(crate::ssr::Ssr::material_target);
        let (prepass, screen_declaration) =
            self.prefix_declarations(source, &mut passes, &mut views)?;
        let mut geometry = self.prefix_geometry(gpu, pipelines, frame, source)?;
        let mut screen = crate::ssao::passes::ScreenFeature {
            ssao: source,
            material,
            frame: *frame,
        };
        self.chain.encode_geometry_prefix(
            device,
            encoder,
            &worldrunner_render_post::FramePlan {
                passes: &passes,
                views: &views,
                requested: &[],
                operations: &[],
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

    fn prefix_geometry<'a>(
        &self,
        gpu: &'a mut crate::draw::SceneGpu,
        pipelines: &'a crate::renderer::ScenePipelines,
        frame: &crate::frame::FrameUniform,
        source: &crate::scene_metadata::SceneMetadata,
    ) -> Result<super::SceneFeature<'a>, FrameError> {
        let depth_mode = prefix_depth(gpu, source);
        let screen = source.geometry_for_depth(depth_mode);
        super::scene::prepare_terrain(gpu, frame, pipelines, Some(&screen))?;
        Ok(super::SceneFeature {
            publications: self.draw_publications(),
            gpu,
            pipelines,
            frame: *frame,
            screen: Some(screen),
            programs: None,
            deferred: None,
            renders: self
                .gbuffer
                .as_ref()
                .is_some_and(crate::gbuffer::Gbuffer::renders),
            opaque_drawn: std::cell::Cell::new(false),
        })
    }

    fn prefix_declarations(
        &self,
        source: &crate::scene_metadata::SceneMetadata,
        passes: &mut Vec<worldrunner_render_post::PassDeclaration>,
        views: &mut Vec<worldrunner_render_post::AttachmentView>,
    ) -> Result<(FeaturePass, FeatureDeclaration), FrameError> {
        let material = self.ssr.as_ref().map(crate::ssr::Ssr::material_target);
        crate::ssao::passes::prepass(source, material, passes, views)?;
        let screen = source.passes.schedule_metadata(passes, views)?;
        if let Some(gbuffer) = self.gbuffer.as_ref().filter(|gbuffer| gbuffer.renders()) {
            let (producer, _) = source.geometry.gbuffer.as_ref().expect("early producer");
            let velocity = self.velocity_attachment(producer.writes_velocity())?;
            gbuffer.fuse_prepass(passes, views, velocity.as_ref())?;
            return Ok((
                crate::gbuffer::producer_pass(producer, source.depth.format()),
                screen,
            ));
        }
        let mut prepass = FeaturePass::new(PassSlot::DepthNormalPrepass);
        prepass.color_format = Some(wgpu::TextureFormat::Rgba16Float);
        prepass.depth_format = Some(source.depth.format());
        prepass.additional_color_formats = passes
            .iter()
            .find(|pass| pass.slot == PassSlot::DepthNormalPrepass)
            .expect("prepass declaration")
            .additional_colors
            .iter()
            .map(|attachment| attachment.format)
            .collect();
        Ok((prepass, screen))
    }

    pub(crate) fn capture_effect_keys(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<Vec<worldrunner_render_gpu::submission::capture::WriteKey>, FrameError> {
        if (self.water.is_some() || self.ssr.is_some() || self.overlay.needs_depth())
            && self.snapshots.is_none()
        {
            self.snapshots = Some(std::array::from_fn(|_| {
                crate::water::snapshots::Snapshots::new(device, self.targets().config())
            }));
        }
        let velocity = self.targets().velocity_texture().cloned();
        let mut textures = Vec::new();
        textures.extend(velocity.as_ref());
        if let Some(screen) = &self.ssao {
            textures.extend(screen.passes.textures());
            textures.extend([&screen.depth, &screen.normals]);
            if let Some(receiver) = &screen.receiver {
                textures.extend(receiver.textures.iter());
            }
        }
        if let Some(water) = &self.water {
            textures.extend(water.receivers[self.frame_slot].iter());
        }
        if let Some(ssr) = &self.ssr {
            textures.extend(ssr.textures());
        }
        if let Some(gbuffer) = &self.gbuffer {
            textures.extend(gbuffer.written_textures());
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
            super::publication::key(&self.temporal),
        ]
    }
}
