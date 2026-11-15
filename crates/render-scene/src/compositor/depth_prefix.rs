use super::{FrameRenderer, reborrow_features};
use std::borrow::Cow;
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, FeaturePass, FrameError, FramePlan,
    PassDeclaration, PassSlot,
};

/// The depth and normal prepass and the first depth level of a frame with screen-space
/// metadata, ready to encode ahead of the rest of the frame.
///
/// [`plan`](Self::plan) is exactly what the prefix completes: the prepass entry
/// (`PassEntry::new(PassSlot::DepthNormalPrepass)`) and the first depth level
/// (`PassEntry::new(PassSlot::DepthPyramid(0))`), both occurrence 0, with the views they attach.
/// The prepass depth attachment holds the device depth the frame's opaque pass reuses. A renderer
/// with a G-buffer path that renders from it fuses the G-buffer producer into the prepass: the
/// same entry then also attaches the G-buffer targets and any velocity target, and the plan lists
/// their views. A caller feature that reads one of these declares that resource in a later entry
/// and finds the view through `PrepareContext::resource`. The suffix frame must list the same
/// passes and attachments for the entries to count as completed; one that does not is refused.
///
/// The prefix keeps no borrow of the renderer or the scene. It describes the metadata and the
/// G-buffer path the renderer held when it was made and is for one frame.
#[derive(Debug)]
pub struct StockDepthPrefix {
    source: crate::scene_metadata::SceneMetadata,
    material: Option<wgpu::Texture>,
    passes: Vec<PassDeclaration>,
    views: Vec<AttachmentView>,
    prepass: FeaturePass,
    screen: FeatureDeclaration,
    depth_mode: crate::renderer::depth::DepthMode,
    renders: bool,
}
impl StockDepthPrefix {
    fn new(
        renderer: &FrameRenderer,
        source: crate::scene_metadata::SceneMetadata,
        relief: bool,
    ) -> Result<Self, FrameError> {
        let mut passes = Vec::new();
        let mut views = Vec::new();
        let (prepass, screen) = renderer.prefix_declarations(&source, &mut passes, &mut views)?;
        let depth_mode = prefix_depth(relief, &source);
        let material = renderer
            .ssr
            .as_ref()
            .map(|ssr| ssr.material_target().clone());
        let renders = renderer
            .gbuffer
            .as_ref()
            .is_some_and(crate::gbuffer::Gbuffer::renders);
        Ok(Self {
            source,
            material,
            passes,
            views,
            prepass,
            screen,
            depth_mode,
            renders,
        })
    }

    /// The prefix passes and the views they attach. Clone them to build the suffix plan.
    #[must_use]
    pub fn plan(&self) -> FramePlan<'_> {
        FramePlan {
            passes: &self.passes,
            views: &self.views,
            requested: &[],
        }
    }

    /// Encodes the prefix, with `contribution`'s passes, views and features appended to it
    /// before the whole is validated. The contribution is checked like any other frame input:
    /// it may not repeat an entry, and a read it declares needs an earlier producer in the
    /// combined list.
    ///
    /// Submission and abandonment follow [`FrameRenderer`]: the prefix is complete only in
    /// the frame of `encoder`, and the suffix must encode in that frame.
    ///
    /// # Errors
    /// Refusals of preparation and of validation, as [`FrameRenderer::encode_frame`].
    pub fn encode(
        &self,
        renderer: &mut FrameRenderer,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        frame: crate::frame::FrameUniform,
        contribution: (&FramePlan<'_>, &mut [BorrowedFeature<'_>]),
    ) -> Result<(), FrameError> {
        let frame = renderer.prepare_uniform(frame)?;
        renderer.encode_prefix(device, encoder, scene, &frame, (self, Some(contribution)))
    }
}

fn prefix_depth(
    relief: bool,
    source: &crate::scene_metadata::SceneMetadata,
) -> crate::renderer::depth::DepthMode {
    use crate::renderer::depth::DepthMode;
    if !source.depth.format().has_stencil_aspect() {
        DepthMode::Independent
    } else if relief && source.receiver.is_none() {
        DepthMode::Shared
    } else {
        DepthMode::Reuse
    }
}

impl FrameRenderer {
    /// The write keys of `textures` in this renderer's capture registry, for a capture's write
    /// contract.
    ///
    /// # Errors
    /// [`FrameError`] when the renderer's registry refuses a texture.
    pub fn capture_texture_keys(
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

    pub(crate) fn draw_publications(&self) -> crate::draw::DrawPublications {
        crate::draw::DrawPublications::with_producer(self.producer_draw.clone())
    }

    /// The stock depth prefix of the frame `gpu` is drawn in. It prepares the renderer's inputs
    /// from the scene first, as [`prepare_stock_scene`](super::prepare_stock_scene) does, then
    /// plans the prefix from the screen-space metadata and the G-buffer path the renderer holds
    /// and from whether the scene draws relief.
    ///
    /// # Errors
    /// `FrameError::UnsupportedEffect("stock depth prefix needs screen-space metadata")` when the
    /// renderer has none: no SSAO, reflections or depth fitting were configured.
    pub fn stock_depth_prefix(
        &mut self,
        device: &wgpu::Device,
        gpu: &crate::draw::SceneGpu,
    ) -> Result<StockDepthPrefix, FrameError> {
        if self.ssao.is_none() {
            return Err(FrameError::UnsupportedEffect(
                "stock depth prefix needs screen-space metadata",
            ));
        }
        self.prepare_scene_inputs(device, gpu);
        let source = self.ssao.as_ref().expect("metadata checked").clone();
        StockDepthPrefix::new(self, source, gpu.has_relief())
    }

    pub(crate) fn encode_depth_prefix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        frame: &crate::frame::FrameUniform,
        source: &crate::scene_metadata::SceneMetadata,
    ) -> Result<(), FrameError> {
        let prefix = StockDepthPrefix::new(self, source.clone(), scene.0.has_relief())?;
        self.encode_prefix(device, encoder, scene, frame, (&prefix, None))
    }

    fn encode_prefix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        frame: &crate::frame::FrameUniform,
        planned: (
            &StockDepthPrefix,
            Option<(&FramePlan<'_>, &mut [BorrowedFeature<'_>])>,
        ),
    ) -> Result<(), FrameError> {
        let (prefix, contribution) = planned;
        let (gpu, pipelines) = scene;
        gpu.prepare_dynamic(encoder, pipelines)?;
        gpu.prepare_terrain_geometry(device, encoder, frame, prefix.source.size)?;
        gpu.prepare_depth_prefix_pages(device, encoder, frame, prefix.source.size)?;
        let mut passes = prefix.passes.clone();
        let mut views = prefix.views.clone();
        let mut requested = Vec::new();
        let mut extra = None;
        if let Some((plan, features)) = contribution {
            passes.extend_from_slice(plan.passes);
            views.extend_from_slice(plan.views);
            requested.extend_from_slice(plan.requested);
            extra = Some(features);
        }
        let mut geometry = self.prefix_geometry(gpu, pipelines, frame, prefix)?;
        let mut screen = crate::ssao::passes::ScreenFeature {
            ssao: &prefix.source,
            material: prefix.material.as_ref(),
            frame: *frame,
        };
        let mut features = vec![
            BorrowedFeature {
                declaration: FeatureDeclaration {
                    passes: vec![prefix.prepass.clone()],
                    effects: vec![],
                },
                prepared: &mut geometry,
            },
            BorrowedFeature {
                declaration: prefix.screen.clone(),
                prepared: &mut screen,
            },
        ];
        if let Some(extra) = extra {
            features.extend(reborrow_features(extra));
        }
        self.chain.encode_geometry_prefix(
            device,
            encoder,
            &FramePlan {
                passes: &passes,
                views: &views,
                requested: &requested,
            },
            &mut features,
        )
    }

    fn prefix_geometry<'a>(
        &self,
        gpu: &'a mut crate::draw::SceneGpu,
        pipelines: &'a crate::renderer::ScenePipelines,
        frame: &crate::frame::FrameUniform,
        prefix: &StockDepthPrefix,
    ) -> Result<super::SceneFeature<'a>, FrameError> {
        let screen = prefix.source.geometry_for_depth(prefix.depth_mode);
        super::scene::prepare_terrain(gpu, frame, pipelines, Some(&screen))?;
        Ok(super::SceneFeature {
            publications: self.draw_publications(),
            gpu,
            pipelines: Cow::Borrowed(pipelines),
            frame: *frame,
            screen: Some(screen),
            programs: None,
            deferred: None,
            renders: prefix.renders,
            opaque_drawn: std::cell::Cell::new(false),
        })
    }

    /// Declares the prefix passes of `source` into `passes` and `views`. With a G-buffer path
    /// that renders, the producer is fused into the prepass: the prepass declaration then
    /// attaches the G-buffer targets and the feature's pass is the producer's.
    fn prefix_declarations(
        &self,
        source: &crate::scene_metadata::SceneMetadata,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
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

    /// The writes of the screen-space and snapshot targets a frame's suffix produces, for a
    /// capture's write contract. Allocates the opaque snapshots a water, reflection or overlay
    /// frame needs.
    ///
    /// # Errors
    /// As [`Self::capture_texture_keys`].
    pub fn capture_effect_keys(
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
