use super::offscreen::encode_player_work;
use super::{FrameRenderer, attachment, attachment_resource};
use std::borrow::Cow;
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, FrameError, FramePlan, PassDeclaration,
    PassSlot, PreparedFeature,
};

pub(crate) struct SceneFeature<'a> {
    pub(crate) publications: crate::draw::DrawPublications,
    pub gpu: &'a mut crate::draw::SceneGpu,
    /// Borrowed by the depth prefix, which selects nothing; owned by the stock scene, which
    /// selects its pipelines for the frame's depth mode and outlives the renderer borrow.
    pub pipelines: Cow<'a, crate::renderer::ScenePipelines>,
    pub frame: crate::frame::FrameUniform,
    pub screen: Option<crate::scene_metadata::Geometry>,
    /// Admitted player program draws, recorded after the stock opaque draw. Present only
    /// without screen-space passes, so the early returns below never skip them.
    pub programs: Option<worldrunner_render_program::runtime::AdmittedPrograms>,
    /// Set 1 of the deferred lighting pass, present only with a G-buffer path.
    pub deferred: Option<crate::gbuffer::lighting::LightingInputs>,
    /// Whether the frame renders from the G-buffer: terrain and objects are stored and lit
    /// before Opaque, which draws only the rest.
    pub renders: bool,
    /// Metadata-only prefixes retain the previous colour pass's published draw results.
    pub opaque_drawn: std::cell::Cell<bool>,
}
impl SceneFeature<'_> {
    pub fn declaration(pipelines: &crate::renderer::ScenePipelines) -> FeatureDeclaration {
        let mut declaration = FeatureDeclaration::new(PassSlot::Opaque);
        declaration.passes[0].color_format = Some(pipelines.color_format);
        declaration.passes[0].samples = pipelines.samples;
        declaration.passes[0].depth_format = Some(pipelines.depth.format());
        declaration
    }
}
impl PreparedFeature for SceneFeature<'_> {
    fn prepare(
        &mut self,
        _context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), FrameError> {
        Ok(())
    }
    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.opaque_drawn.set(false);
        self.publications.install(
            context,
            crate::draw::PassInstallation::Absent,
            self.screen.is_some(),
            self.gpu.ssao,
        );
        self.gpu.prepare_instances(context);
        self.gpu.commit_frame(context, self.frame);
        self.gpu.prepare_view_compaction(context, None, &self.frame);
    }
    fn finish_draws(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        if self.opaque_drawn.get() {
            self.publications.publish(context, self.gpu);
        }
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        if context.slot == PassSlot::Celestial {
            self.gpu.draw_celestial(pass);
            return;
        }
        #[cfg(test)]
        crate::scene_metadata::record_pass(context.slot);
        if context.slot == PassSlot::OpaqueLighting {
            let inputs = self.deferred.as_ref().expect("declared lighting pass");
            let pipelines = self.colour_pipelines(pass);
            self.gpu.draw_deferred_lighting(pass, pipelines, inputs);
            return;
        }
        if self.draw_metadata(context.slot, pass) {
            return;
        }
        self.begin_colour(context.slot);
        if let Some(screen) = &self.screen {
            if let Some((pipelines, group)) = &screen.forward {
                pass.set_bind_group(3, group, &[]);
                if self.renders {
                    self.publications.record_remainder(
                        self.gpu,
                        self.gpu.draw_forward_remainder(pass, pipelines),
                    );
                    return;
                }
                self.publications
                    .record_color(self.gpu, self.gpu.draw_color(pass, pipelines, None));
                return;
            }
            pass.set_bind_group(3, &screen.ambient, &[]);
        }
        if self.renders {
            self.publications.record_remainder(
                self.gpu,
                self.gpu.draw_forward_remainder(pass, &self.pipelines),
            );
            return;
        }
        self.publications
            .record_color(self.gpu, self.gpu.draw_color(pass, &self.pipelines, None));
        if let Some(programs) = self
            .programs
            .as_ref()
            .filter(|_| context.slot == PassSlot::Opaque)
        {
            programs.record(pass);
        }
    }
}

impl SceneFeature<'_> {
    fn begin_colour(&self, slot: PassSlot) {
        if slot == PassSlot::Opaque {
            self.opaque_drawn.set(true);
            self.gpu
                .reset_pass_statistics(self.publications.prepass_ran());
        }
    }

    fn draw_metadata(&self, slot: PassSlot, pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some(screen) = &self.screen else {
            return false;
        };
        if slot == PassSlot::OpaqueGbuffer || (self.renders && slot == PassSlot::DepthNormalPrepass)
        {
            if self.renders {
                self.publications.prepass();
            }
            if let Some((pipelines, group)) = &screen.gbuffer {
                let draw = self.gpu.draw_gbuffer(pass, pipelines, group);
                if self.renders {
                    let ao = colour_pipelines(&self.pipelines, Some(screen)).has_ao_receiver();
                    self.publications.record_gbuffer(draw, false, ao);
                }
            }
            return true;
        }
        if slot == PassSlot::DepthNormalPrepass {
            self.publications.prepass();
            self.publications.record_prepass(
                self.gpu,
                self.gpu
                    .draw_prepass(pass, screen.prepass_pipelines(), &screen.group),
            );
            return true;
        }
        false
    }

    /// The pipelines the colour pass draws with, after binding set 3 as that pass does.
    fn colour_pipelines(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
    ) -> &crate::renderer::ScenePipelines {
        let Some(screen) = &self.screen else {
            return &self.pipelines;
        };
        if let Some((pipelines, group)) = &screen.forward {
            pass.set_bind_group(3, group, &[]);
            return pipelines;
        }
        pass.set_bind_group(3, &screen.ambient, &[]);
        &self.pipelines
    }
}

/// The pipelines a stock feature's colour pass draws with: the forward reflection pipelines
/// when installed, otherwise `pipelines`.
pub(crate) fn colour_pipelines<'a>(
    pipelines: &'a crate::renderer::ScenePipelines,
    screen: Option<&'a crate::scene_metadata::Geometry>,
) -> &'a crate::renderer::ScenePipelines {
    screen
        .and_then(|screen| screen.forward.as_ref())
        .map_or(pipelines, |(forward, _)| forward)
}

/// Prepares the terrain and object pipelines of the passes the scene feature draws them in: the
/// prepass when the frame has screen-space passes, and the colour pass with the forward
/// pipelines when those are installed. The celestial bodies' pipelines are prepared too. Preparing is done before the passes open, where a
/// refusal is still an error.
pub(super) fn prepare_terrain(
    gpu: &crate::draw::SceneGpu,
    frame: &crate::frame::FrameUniform,
    pipelines: &crate::renderer::ScenePipelines,
    screen: Option<&crate::scene_metadata::Geometry>,
) -> Result<(), FrameError> {
    gpu.prepare_celestial(frame)?;
    let Some(screen) = screen else {
        gpu.prepare_terrain(pipelines, frame)?;
        return gpu.prepare_objects(pipelines, frame);
    };
    gpu.prepare_terrain(screen.prepass_pipelines(), frame)?;
    gpu.prepare_objects(screen.prepass_pipelines(), frame)?;
    if let Some((gbuffer, _)) = &screen.gbuffer {
        gpu.prepare_terrain(gbuffer, frame)?;
        gpu.prepare_objects(gbuffer, frame)?;
    }
    let colour = screen
        .forward
        .as_ref()
        .map_or(pipelines, |(forward, _)| forward);
    gpu.prepare_terrain(colour, frame)?;
    gpu.prepare_objects(colour, frame)
}

/// Adds the velocity attachment, which the main pass clears to zero and never resolves.
pub(crate) fn declare_velocity(
    opaque: &mut PassDeclaration,
    views: &mut Vec<AttachmentView>,
    velocity: Option<&AttachmentView>,
) -> Result<(), FrameError> {
    if let Some(velocity) = velocity {
        let id = attachment_resource("scene velocity", velocity);
        opaque
            .additional_colors
            .push(attachment(id, velocity, false));
        views.push(velocity.with_resource(id)?);
    }
    Ok(())
}

/// Checks the screen-space, water and post settings the scene was uploaded with against the
/// renderer.
fn validate_effects(
    renderer: &mut FrameRenderer,
    gpu: &crate::draw::SceneGpu,
    pipelines: &crate::renderer::ScenePipelines,
) -> Result<(), FrameError> {
    renderer.validate_ssao(pipelines, gpu.ssao.as_ref())?;
    renderer.validate_water(&gpu.water)?;
    renderer.validate_ssr(gpu.ssr.as_ref())?;
    renderer.validate_post(gpu.post)
}

/// The stock opaque scene of one frame, prepared for composition with caller features.
///
/// It owns the pipelines it selected for the frame and holds the resident scene exclusively
/// until it is dropped; it holds no borrow of the [`FrameRenderer`] that prepared it. Its
/// [`plan`](Self::plan) is the stock passes and attachments, which a caller extends with its own
/// before encoding, and every read a caller feature declares of the stock outputs is validated
/// by the same checks as the stock passes' own.
///
/// ```ignore
/// let mut stock = prepare_stock_scene(renderer, device, encoder, scene, attachments, frame)?;
/// let plan = stock.plan();
/// let (mut passes, mut views) = (plan.passes.to_vec(), plan.views.to_vec());
/// let (declaration, uniform) = (stock.declaration(), stock.frame());
/// passes.push(caller_pass);
/// views.push(caller_view);
/// renderer.encode_frame(
///     device,
///     encoder,
///     (&passes, &views),
///     &mut [
///         BorrowedFeature { declaration, prepared: &mut stock },
///         BorrowedFeature { declaration: caller_declaration, prepared: &mut caller },
///     ],
///     (uniform, None),
/// )?;
/// ```
pub struct StockSceneFeature<'a> {
    feature: SceneFeature<'a>,
    declaration: FeatureDeclaration,
    passes: Vec<PassDeclaration>,
    views: Vec<AttachmentView>,
}
impl StockSceneFeature<'_> {
    /// The passes the stock scene draws in, with the formats of its attachments.
    #[must_use]
    pub fn declaration(&self) -> FeatureDeclaration {
        self.declaration.clone()
    }

    /// The stock passes and the attachment views they name, in the frame's authored order. The
    /// attachments of every pass are declared here, so a caller reads a stock output by
    /// declaring a read of the resource a pass here produces.
    #[must_use]
    pub fn plan(&self) -> FramePlan<'_> {
        FramePlan {
            passes: &self.passes,
            views: &self.views,
            requested: &[],
        }
    }

    /// The frame uniform the renderer prepared, with exposure and jitter applied. Pass this to
    /// `encode_frame`; the uniform given to [`prepare_stock_scene`] has neither applied yet.
    #[must_use]
    pub fn frame(&self) -> crate::frame::FrameUniform {
        self.feature.frame
    }
}
impl PreparedFeature for StockSceneFeature<'_> {
    fn prepare(
        &mut self,
        context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), FrameError> {
        self.feature.prepare(context)
    }
    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.feature.commit(context);
    }
    fn finish_draws(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.feature.finish_draws(context);
    }
    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        self.feature.draw(context, pass);
    }
}

struct SceneDraw<'a, 'b> {
    gpu: &'a mut crate::draw::SceneGpu,
    pipelines: &'b crate::renderer::ScenePipelines,
    color: &'b AttachmentView,
    depth: &'b AttachmentView,
    frame: crate::frame::FrameUniform,
    screen: Option<crate::scene_metadata::Geometry>,
    velocity: Option<AttachmentView>,
    programs: Option<worldrunner_render_program::runtime::AdmittedPrograms>,
    depth_mode: crate::renderer::depth::DepthMode,
}

struct ScenePassPlan<'b> {
    depth_id: worldrunner_render_post::ResourceRef,
    depth_texture: &'b wgpu::Texture,
    declaration: FeatureDeclaration,
    passes: Vec<PassDeclaration>,
    views: Vec<AttachmentView>,
}

fn declare_scene_passes<'b>(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    scene: &mut SceneDraw<'_, 'b>,
) -> Result<ScenePassPlan<'b>, FrameError> {
    let SceneDraw {
        gpu,
        pipelines,
        color,
        depth,
        velocity,
        depth_mode,
        ..
    } = scene;
    let color_id = attachment_resource("scene color", color);
    let depth_id = renderer.scene_depth_resource(depth);
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(renderer.color_attachment(color_id, color));
    opaque.depth = Some(attachment(depth_id, depth, true));
    if *depth_mode == crate::renderer::depth::DepthMode::Reuse {
        opaque.depth.as_mut().expect("scene depth").load =
            worldrunner_render_post::AttachmentLoad::Load;
    }
    let mut declaration = SceneFeature::declaration(pipelines);
    if let Some(screen) = &renderer.ssao {
        crate::ssao::passes::geometry_declaration(&mut declaration, screen);
        crate::gbuffer::declare_producer(&mut declaration, screen, *depth_mode);
    }
    let mut views = vec![
        color.with_resource(color_id)?,
        depth.with_resource(depth_id)?,
    ];
    declare_velocity(&mut opaque, &mut views, velocity.as_ref())?;
    let mut passes = vec![opaque];
    gpu.plan_celestial(
        device,
        color.size(),
        &mut passes,
        &mut views,
        &mut declaration,
    );
    Ok(ScenePassPlan {
        depth_id,
        depth_texture: depth.texture(),
        declaration,
        passes,
        views,
    })
}

/// Prepares a resident unshadowed scene as a feature that callers compose with their own.
/// Everything [`encode_scene`] does before encoding happens here, in the same order: depth and
/// effect validation, the player's offscreen work, dynamic and terrain preparation, the stock
/// plan and the deferred lighting inputs. Checked attachments retain their selected mip, array
/// layer and view format.
///
/// The submission and abandonment rules of [`FrameRenderer`] apply, and the scene's dynamic
/// uploads are staged into `encoder` now, so the feature must be encoded in that
/// encoder's frame or the frame dropped.
///
/// # Errors
/// As [`encode_scene`].
pub fn prepare_stock_scene<'a>(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (
        &'a mut crate::draw::SceneGpu,
        &crate::renderer::ScenePipelines,
    ),
    attachments: (&AttachmentView, &AttachmentView),
    frame: crate::frame::FrameUniform,
) -> Result<StockSceneFeature<'a>, FrameError> {
    let (gpu, pipelines) = scene;
    renderer.validate_opaque_depth(attachments.1)?;
    renderer.validate_taa_depth(attachments.1)?;
    validate_effects(renderer, gpu, pipelines)?;
    if let Some(screen) = &mut renderer.ssao {
        screen.receiver = None;
    }
    renderer.prepare_scene_inputs(device, gpu);
    let depth_mode = renderer.forward_depth(gpu, attachments.1);
    let selected = renderer.color_pipelines(pipelines, depth_mode);
    let pipelines = &selected;
    let writes_velocity = renderer.adopt_scene_velocity(device, gpu, pipelines);
    let velocity = renderer.velocity_attachment(writes_velocity)?;
    let programs = encode_player_work(gpu, encoder, renderer, (pipelines, velocity.is_some()))?;
    gpu.prepare_dynamic(encoder, pipelines)?;
    let frame = renderer.prepare_uniform(frame)?;
    let (color, depth) = attachments;
    gpu.prepare_terrain_geometry(device, encoder, &frame, color.size())?;
    gpu.prepare_sky(device, encoder, &frame)?;
    gpu.prepare_terrain_pages(device, encoder, &frame, color.size())?;
    let screen = renderer.geometry_for_depth(depth_mode);
    prepare_terrain(gpu, &frame, pipelines, screen.as_ref())?;
    let mut draw = SceneDraw {
        gpu,
        pipelines,
        color,
        depth,
        frame,
        screen,
        velocity,
        programs,
        depth_mode,
    };
    let plan = declare_scene_passes(renderer, device, &mut draw)?;
    let SceneDraw {
        gpu,
        pipelines,
        frame,
        screen,
        programs,
        depth_mode,
        ..
    } = draw;
    let ScenePassPlan {
        depth_id,
        depth_texture,
        mut declaration,
        passes,
        views,
    } = plan;
    let deferred = renderer.prepare_deferred_lighting(
        device,
        (gpu, colour_pipelines(pipelines, screen.as_ref())),
        (
            depth_id,
            depth_texture,
            depth_mode == crate::renderer::depth::DepthMode::Reuse,
        ),
        &frame,
        &mut declaration,
    )?;
    Ok(StockSceneFeature {
        feature: SceneFeature {
            publications: renderer.draw_publications(),
            screen,
            programs,
            gpu,
            pipelines: Cow::Owned(selected),
            frame,
            deferred,
            renders: renderer
                .gbuffer
                .as_ref()
                .is_some_and(crate::gbuffer::Gbuffer::renders),
            opaque_drawn: std::cell::Cell::new(false),
        },
        declaration,
        passes,
        views,
    })
}

/// Encodes a resident unshadowed scene through the feature boundary.
/// Checked attachments retain their selected mip, array layer and view format.
/// The submission and abandonment rules of [`FrameRenderer`] apply.
///
/// This is [`prepare_stock_scene`] encoded alone: callers that add passes or features of their
/// own prepare the stock scene themselves and call [`FrameRenderer::encode_frame`].
pub fn encode_scene(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
    attachments: (&AttachmentView, &AttachmentView),
    frame: crate::frame::FrameUniform,
) -> Result<(), FrameError> {
    let mut stock = prepare_stock_scene(renderer, device, encoder, scene, attachments, frame)?;
    let plan = stock.plan();
    let (passes, views) = (plan.passes.to_vec(), plan.views.to_vec());
    let (declaration, frame) = (stock.declaration(), stock.frame());
    renderer.encode_frame(
        device,
        encoder,
        (&passes, &views),
        &mut [BorrowedFeature {
            declaration,
            prepared: &mut stock,
        }],
        (frame, None),
    )
}
