use super::FrameRenderer;
use std::borrow::Cow;
use worldrunner_render_post::AttachmentView;
use worldrunner_render_post::BorrowedFeature;
use worldrunner_render_post::FeatureDeclaration;
use worldrunner_render_post::FrameError;
use worldrunner_render_post::FramePlan;
use worldrunner_render_post::GamePlace;
use worldrunner_render_post::PassDeclaration;
use worldrunner_render_post::PassEntry;
use worldrunner_render_post::PassSlot;

/// A frame's pass list and the views it attaches.
type ListedPlan = (Vec<PassDeclaration>, Vec<AttachmentView>);

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
/// The prepass depth attachment holds the device depth the frame's opaque pass reuses. A caller
/// feature that reads one of the prefix's products declares that resource in a later entry and
/// finds the view through `PrepareContext::resource`.
///
/// The suffix frame decides which prefix entries count as completed. Each prefix entry the
/// suffix also lists counts as completed when it has the same colour, extra colour and depth
/// attachments and every one binds the same texture; otherwise the frame is refused with
/// `FrameError::AttachmentContract` or `FrameError::AttachmentView` naming it. A prefix entry
/// the suffix does not list is skipped, not refused, so a suffix that reads only the depth need
/// not repeat the metadata entries.
///
/// The prefix keeps no borrow of the renderer or the scene. It holds the metadata, the target
/// views and the velocity choice the renderer had when it was made, and is for
/// one frame: after a change of effects, targets or size, make a new one with
/// [`FrameRenderer::stock_depth_prefix`].
#[derive(Debug)]
pub struct StockDepthPrefix {
    source: crate::scene_metadata::SceneMetadata,
    material: Option<wgpu::Texture>,
    schedule: super::stock_plan::LogicalPrefix,
    views: Vec<AttachmentView>,
    depth_mode: crate::renderer::depth::DepthMode,
}
/// The schedule of a prefix and the views its attachments select.
struct PrefixParts {
    schedule: super::stock_plan::LogicalPrefix,
    views: Vec<AttachmentView>,
}

impl StockDepthPrefix {
    fn new(
        renderer: &FrameRenderer,
        source: crate::scene_metadata::SceneMetadata,
        relief: bool,
    ) -> Result<Self, FrameError> {
        let (schedule, views) = renderer.derive_prefix(&source)?;
        let plan = PrefixParts { schedule, views };
        Ok(Self::from_plan(renderer, source, relief, plan))
    }

    fn from_plan(
        renderer: &FrameRenderer,
        source: crate::scene_metadata::SceneMetadata,
        relief: bool,
        plan: PrefixParts,
    ) -> Self {
        let PrefixParts { schedule, views } = plan;
        let depth_mode = prefix_depth(relief, &source);
        let material = renderer
            .ssr
            .as_ref()
            .map(|ssr| ssr.material_target().clone());
        Self {
            source,
            material,
            schedule,
            views,
            depth_mode,
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

    /// The stock depth prefix of the frame `scene` is drawn in. It prepares the renderer's inputs
    /// from the scene first, as [`prepare_stock_scene`](super::prepare_stock_scene) does, takes
    /// the same decision on whether the colour draws write velocity, then plans the prefix from
    /// the screen-space metadata the renderer holds and from whether the scene draws relief. `pipelines` are the ones the suffix draws with.
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
        self.adopt_scene_velocity(gpu, pipelines);
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
            PrefixParts {
                schedule: (*plan.schedule).clone(),
                views: (*plan.views).clone(),
            },
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
        // Prefix features prepare after this, so they read the chunks the suffix will lend. The
        // suffix publishes again with its colour target's extent. The frame's terrain is
        // selected once, for the metadata's extent, so a suffix with another extent fails its
        // own terrain preparation (`StreamingSelectionConflict`) before it can publish.
        // The same call lends the resident textures and meshes, which are scene content, not a
        // product of the frame, so the prefix's game passes may sample them.
        self.publish_game_terrain(gpu, frame, prefix.source.size);
        gpu.prepare_depth_prefix_pages(device, encoder, frame, prefix.source.size)?;
        let mut passes = prefix.schedule.passes.clone();
        let mut views = prefix.views.clone();
        let mut requested = Vec::new();
        let mut operations = Vec::new();
        let mut extra = None;
        self.plan_game_prefix(frame, &mut passes, &mut views)?;
        if let Some((plan, features)) = contribution {
            passes.extend_from_slice(plan.passes);
            views.extend_from_slice(plan.views);
            requested.extend_from_slice(plan.requested);
            operations.extend_from_slice(plan.operations);
            extra = Some(features);
        }
        let mut geometry = match Self::prefix_geometry(gpu, pipelines, frame, prefix) {
            Ok(geometry) => geometry,
            Err(error) => {
                self.chain.discard_prefix_plan();
                return Err(error);
            }
        };
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
        )?;
        self.record_game_work(encoder)
    }

    /// Plans the game's passes after the depth prepass into the prefix, immediately after the
    /// prepass and before the depth levels that read it, with the camera of the frame the prefix
    /// belongs to, and with the loads the frame that completes the prefix plans them with. A
    /// renderer whose game has no depth-prefix feature plans nothing.
    ///
    /// # Errors
    /// [`FrameError`] when a depth-prefix feature refuses to plan, and
    /// [`FrameError::MissingPass`] naming the depth prepass when it planned passes into a prefix
    /// that has none. A refused prefix keeps no plan for the next one.
    pub(super) fn plan_game_prefix(
        &mut self,
        frame: &crate::frame::FrameUniform,
        passes: &mut Vec<worldrunner_render_post::PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<(), FrameError> {
        if !self.chain.has_prefix_features() {
            return Ok(());
        }
        self.game_camera
            .set(&super::FrameCamera::of(frame, self.frame_slot));
        let stock = passes.len();
        self.chain.plan_prefix_features(passes, views)?;
        super::game::place_after_prepass(passes, stock)
            .inspect_err(|_| self.chain.discard_prefix_plan())
    }

    /// The caller's passes and views for the rest of a frame whose depth prefix, encoded
    /// separately in `encoder`'s frame, ran game passes after the prepass while the frame's own
    /// plan has no prepass (a renderer without screen-space metadata, whose prefix is the depth
    /// fitting's): `plan` with the prefix's prepass and the views it attaches. The game's passes
    /// after the prepass then plan in the frame as they planned in the prefix, so the frame lists
    /// them as completed and its later passes load what they wrote, as in a frame that plans the
    /// prepass itself. `None` leaves `plan` as it is.
    ///
    /// # Errors
    /// [`FrameError`] when `encoder` is not a checked transaction encoder, and
    /// [`FrameError::MissingPass`] naming the depth prepass when the prefix ran game passes without
    /// one.
    pub(super) fn with_prefix_prepass(
        &self,
        encoder: &wgpu::CommandEncoder,
        plan: (&[PassDeclaration], &[AttachmentView]),
    ) -> Result<Option<ListedPlan>, FrameError> {
        let (passes, views) = plan;
        let lists = |passes: &[PassDeclaration], slot| passes.iter().any(|pass| pass.slot == slot);
        if self.ssao.is_some() || lists(passes, PassSlot::DepthNormalPrepass) {
            return Ok(None);
        }
        let Some(prefix) = self.chain.geometry_prefix_plan(encoder)? else {
            return Ok(None);
        };
        if !lists(prefix.passes, PassSlot::Game(GamePlace::AfterDepthPrepass)) {
            return Ok(None);
        }
        let prepass = prefix
            .passes
            .iter()
            .find(|pass| pass.slot == PassSlot::DepthNormalPrepass)
            .ok_or(FrameError::MissingPass(PassEntry::new(
                PassSlot::DepthNormalPrepass,
            )))?;
        let mut views = views.to_vec();
        for attachment in prepass
            .color
            .iter()
            .chain(&prepass.additional_colors)
            .chain(&prepass.depth)
        {
            if views
                .iter()
                .any(|view| view.resource() == attachment.resource)
            {
                continue;
            }
            let view = prefix
                .views
                .iter()
                .find(|view| view.resource() == attachment.resource)
                .expect("a validated prefix has a view for each attachment");
            views.push(view.clone());
        }
        let mut passes = passes.to_vec();
        passes.push(prepass.clone());
        Ok(Some((passes, views)))
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
        gpu: &'a mut crate::draw::SceneGpu,
        pipelines: &'a crate::renderer::ScenePipelines,
        frame: &crate::frame::FrameUniform,
        prefix: &StockDepthPrefix,
    ) -> Result<super::SceneFeature<'a>, FrameError> {
        let screen = prefix.source.geometry_for_depth(prefix.depth_mode);
        super::scene::prepare_terrain(gpu, frame, pipelines, Some(&screen))?;
        Ok(super::SceneFeature {
            publications: crate::draw::DrawPublications::default(),
            gpu,
            pipelines: Cow::Borrowed(pipelines),
            frame: *frame,
            screen: Some(screen),
            programs: None,
            contour: None,
            opaque_drawn: std::cell::Cell::new(false),
        })
    }

    /// The writes of the screen-space, snapshot and game targets a frame's suffix produces, for a
    /// capture's write contract. Allocates the opaque snapshots a water, reflection or overlay
    /// frame needs. A game's persistent targets are imported at the initial version the frame
    /// imports them at, which is why every capture lists them here, with the publications of
    /// [`Self::capture_write_keys`], rather than as stock textures.
    ///
    /// # Errors
    /// As [`Self::capture_texture_keys`].
    pub fn capture_effect_keys(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<Vec<worldrunner_render_gpu::submission::capture::WriteKey>, FrameError> {
        if self.snapshot_readers() && self.snapshots.is_none() {
            self.snapshots = Some(std::array::from_fn(|_| {
                crate::water::snapshots::Snapshots::new(device, self.targets().config())
            }));
        }
        self.ensure_game_snapshots(device);
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
        if let Some(water) = self.game.replaces.stock_water(self.water.as_ref()) {
            textures.extend(water.receivers[self.frame_slot].iter());
        }
        if let Some(ssr) = &self.ssr {
            textures.extend(ssr.textures());
        }
        if let Some(snapshots) = self.snapshots.as_ref().filter(|_| self.snapshots_written()) {
            let snapshot = &snapshots[self.frame_slot];
            textures.extend([&snapshot.color, &snapshot.depth, &snapshot.final_depth]);
            textures.extend(snapshot.device_depth.as_ref().map(|depth| &depth.texture));
            textures.extend(
                snapshot
                    .final_device_depth
                    .as_ref()
                    .map(|depth| &depth.texture),
            );
        }
        let game = self.game_capture_imports();
        let imports: Vec<_> = textures
            .into_iter()
            .map(|texture| (texture, None))
            .chain(game.iter().map(|(texture, initial)| (texture, *initial)))
            .collect();
        self.chain
            .capture_write_keys_initial(&imports)
            .map_err(FrameError::from)
    }

    pub fn capture_write_keys(&self) -> Vec<worldrunner_render_gpu::submission::capture::WriteKey> {
        let mut keys = vec![
            super::publication::key(&self.snapshot_ready),
            super::publication::key(&self.post_time),
            super::publication::key(&self.exposure_reset),
            super::publication::key(&self.temporal),
            super::publication::key(&self.contours.published),
        ];
        keys.extend(self.game_capture_publications());
        keys
    }
}
