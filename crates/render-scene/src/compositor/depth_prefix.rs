use super::FrameRenderer;
use std::borrow::Cow;
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, FrameError, FramePlan,
};

fn reborrow_features<'a>(features: &'a mut [BorrowedFeature<'_>]) -> Vec<BorrowedFeature<'a>> {
    features
        .iter_mut()
        .map(|feature| BorrowedFeature {
            declaration: feature.declaration.clone(),
            prepared: &mut *feature.prepared,
        })
        .collect()
}

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
/// and finds the view through `PrepareContext::resource`.
///
/// The suffix frame decides which prefix entries count as completed. Each prefix entry the
/// suffix also lists counts as completed when it has the same colour, extra colour and depth
/// attachments and every one binds the same texture; otherwise the frame is refused with
/// `FrameError::AttachmentContract` or `FrameError::AttachmentView` naming it. A prefix entry
/// the suffix does not list is skipped, not refused, so a suffix that reads only the depth need
/// not repeat the metadata entries.
///
/// The prefix keeps no borrow of the renderer or the scene. It holds the metadata, the G-buffer
/// path, the target views and the velocity choice the renderer had when it was made, and is for
/// one frame: after a change of effects, targets or size, make a new one with
/// [`FrameRenderer::stock_depth_prefix`].
#[derive(Debug)]
pub struct StockDepthPrefix {
    source: crate::scene_metadata::SceneMetadata,
    material: Option<wgpu::Texture>,
    schedule: super::stock_plan::LogicalPrefix,
    views: Vec<AttachmentView>,
    depth_mode: crate::renderer::depth::DepthMode,
    renders: bool,
}
impl StockDepthPrefix {
    fn new(
        renderer: &FrameRenderer,
        source: crate::scene_metadata::SceneMetadata,
        relief: bool,
    ) -> Result<Self, FrameError> {
        let plan = renderer.derive_prefix(&source)?;
        Ok(Self::from_plan(renderer, source, relief, plan))
    }

    fn from_plan(
        renderer: &FrameRenderer,
        source: crate::scene_metadata::SceneMetadata,
        relief: bool,
        plan: (super::stock_plan::LogicalPrefix, Vec<AttachmentView>),
    ) -> Self {
        let (schedule, views) = plan;
        let depth_mode = prefix_depth(relief, &source);
        let material = renderer
            .ssr
            .as_ref()
            .map(|ssr| ssr.material_target().clone());
        let renders = renderer
            .gbuffer
            .as_ref()
            .is_some_and(crate::gbuffer::Gbuffer::renders);
        Self {
            source,
            material,
            schedule,
            views,
            depth_mode,
            renders,
        }
    }

    /// The prefix passes and the views they attach. Clone them to build the suffix plan.
    #[must_use]
    pub fn plan(&self) -> FramePlan<'_> {
        FramePlan {
            passes: &self.schedule.passes,
            views: &self.views,
            requested: &[],
            operations: &[],
        }
    }

    /// Encodes the prefix with `contribution`'s passes, views, requested effects, operations
    /// and features included before the whole is validated. The contribution is checked like
    /// any other frame input: it may not repeat an entry, and a read it declares needs an earlier
    /// producer in the combined list.
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

    /// The stock depth prefix of the frame `scene` is drawn in. It prepares the renderer's inputs
    /// from the scene first, as [`prepare_stock_scene`](super::prepare_stock_scene) does, takes
    /// the same decision on whether the colour draws write velocity, then plans the prefix from
    /// the screen-space metadata and the G-buffer path the renderer holds and from whether the
    /// scene draws relief. `pipelines` are the ones the suffix draws with.
    ///
    /// # Errors
    /// `FrameError::UnsupportedEffect("stock depth prefix needs screen-space metadata")` when the
    /// renderer has none: no SSAO, reflections or depth fitting were configured.
    pub fn stock_depth_prefix(
        &mut self,
        device: &wgpu::Device,
        scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
    ) -> Result<StockDepthPrefix, FrameError> {
        let (gpu, pipelines) = scene;
        if self.ssao.is_none() {
            return Err(FrameError::UnsupportedEffect(
                "stock depth prefix needs screen-space metadata",
            ));
        }
        self.prepare_scene_inputs(device, gpu);
        self.adopt_scene_velocity(device, gpu, pipelines);
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
        let plan = self.plan_prefix(source, frame)?;
        let prefix = StockDepthPrefix::from_plan(
            self,
            source.clone(),
            scene.0.has_relief(),
            ((*plan.schedule).clone(), (*plan.views).clone()),
        );
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
        let mut passes = prefix.schedule.passes.clone();
        let mut views = prefix.views.clone();
        let mut requested = Vec::new();
        let mut operations = Vec::new();
        let mut extra = None;
        if let Some((plan, features)) = contribution {
            passes.extend_from_slice(plan.passes);
            views.extend_from_slice(plan.views);
            requested.extend_from_slice(plan.requested);
            operations.extend_from_slice(plan.operations);
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
                    passes: vec![prefix.schedule.prepass.clone()],
                    effects: vec![],
                },
                prepared: &mut geometry,
            },
            BorrowedFeature {
                declaration: prefix.schedule.screen.clone(),
                prepared: &mut screen,
            },
        ];
        if let Some(extra) = extra {
            features.extend(reborrow_features(extra));
        }
        self.encode_prefix_plan(
            device,
            encoder,
            &FramePlan {
                passes: &passes,
                views: &views,
                requested: &requested,
                operations: &operations,
            },
            &mut features,
        )
    }

    fn encode_prefix_plan(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        plan: &FramePlan<'_>,
        features: &mut [BorrowedFeature<'_>],
    ) -> Result<(), FrameError> {
        #[cfg(test)]
        super::plan_trace::record(true, plan.passes, features, plan.operations);
        self.chain
            .encode_geometry_prefix(device, encoder, plan, features)
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
