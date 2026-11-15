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
/// The prepass depth attachment holds the device depth the frame's opaque pass reuses. A caller
/// feature that reads it declares that resource in a later entry and finds the view through
/// `PrepareContext::resource`. The suffix frame must list the same passes and attachments for the
/// entries to count as completed; one that does not is refused.
///
/// The prefix keeps no borrow of the renderer or the scene. It describes the metadata the
/// renderer held when it was made and is for one frame.
#[derive(Debug)]
pub struct StockDepthPrefix {
    source: crate::scene_metadata::SceneMetadata,
    material: Option<wgpu::Texture>,
    passes: Vec<PassDeclaration>,
    views: Vec<AttachmentView>,
    prepass: FeaturePass,
    screen: FeatureDeclaration,
    depth_mode: crate::renderer::depth::DepthMode,
}
impl StockDepthPrefix {
    fn new(
        source: crate::scene_metadata::SceneMetadata,
        material: Option<wgpu::Texture>,
        relief: bool,
    ) -> Result<Self, FrameError> {
        let mut passes = Vec::new();
        let mut views = Vec::new();
        crate::ssao::passes::prepass(&source, material.as_ref(), &mut passes, &mut views)?;
        let screen = source.passes.schedule_metadata(&mut passes, &mut views)?;
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
        let depth_mode = if source.depth.format().has_stencil_aspect() {
            if relief && source.receiver.is_none() {
                crate::renderer::depth::DepthMode::Shared
            } else {
                crate::renderer::depth::DepthMode::Reuse
            }
        } else {
            crate::renderer::depth::DepthMode::Independent
        };
        Ok(Self {
            source,
            material,
            passes,
            views,
            prepass,
            screen,
            depth_mode,
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

    /// The stock depth prefix of the frame `gpu` is drawn in. It prepares the renderer's inputs
    /// from the scene first, as [`prepare_stock_scene`](super::prepare_stock_scene) does, then
    /// plans the prefix from the screen-space metadata the renderer holds and from whether the
    /// scene draws relief.
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
        let source = self.ssao.as_ref().expect("metadata checked");
        StockDepthPrefix::new(
            source.clone(),
            self.ssr.as_ref().map(|ssr| ssr.material_target().clone()),
            gpu.has_relief(),
        )
    }

    pub(crate) fn encode_depth_prefix(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        frame: &crate::frame::FrameUniform,
        source: &crate::scene_metadata::SceneMetadata,
    ) -> Result<(), FrameError> {
        let material = self.ssr.as_ref().map(|ssr| ssr.material_target().clone());
        let prefix = StockDepthPrefix::new(source.clone(), material, scene.0.has_relief())?;
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
        let mut geometry = super::SceneFeature {
            publications: crate::draw::DrawPublications::default(),
            gpu,
            pipelines: Cow::Borrowed(pipelines),
            frame: *frame,
            screen: Some(prefix.source.geometry_for_depth(prefix.depth_mode)),
            programs: None,
        };
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
            textures
                .extend(crate::gbuffer::GbufferTarget::ALL.map(|target| gbuffer.texture(target)));
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
