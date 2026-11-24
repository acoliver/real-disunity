use super::offscreen::encode_player_work;
use super::{FrameRenderer, attachment, attachment_resource};
use std::borrow::Cow;
use worldrunner_render_post::{
    AttachmentView, FeatureDeclaration, FrameError, FramePlan, PassDeclaration, PassSlot,
    PreparedFeature,
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
    pub contour: Option<crate::draw::ContourDraw>,
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
        if context.slot == PassSlot::ContourIds {
            self.gpu
                .draw_contours(pass, self.contour.as_ref().expect("declared contour pass"));
            return;
        }
        if context.slot == PassSlot::Celestial {
            self.gpu.draw_celestial(pass);
            return;
        }
        #[cfg(test)]
        crate::scene_metadata::record_pass(context.slot);
        if self.draw_metadata(context.slot, pass) {
            return;
        }
        self.begin_colour(context.slot);
        if let Some(screen) = &self.screen {
            if let Some((pipelines, group)) = &screen.forward {
                pass.set_bind_group(3, group, &[]);
                self.publications
                    .record_color(self.gpu, self.gpu.draw_color(pass, pipelines, None));
                return;
            }
            pass.set_bind_group(3, &screen.ambient, &[]);
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
/// let mut stock = prepare_stock_scene(
///     renderer,
///     device,
///     encoder,
///     scene,
///     crate::compositor::ColorDepth { color, depth },
///     frame,
/// )?;
/// let plan = stock.plan();
/// let (mut passes, mut views) = (plan.passes.to_vec(), plan.views.to_vec());
/// let (declaration, uniform) = (stock.declaration(), stock.frame());
/// passes.push(caller_pass);
/// views.push(caller_view);
/// let offscreen = stock.take_offscreen_operations();
/// renderer.encode_frame(
///     device,
///     encoder,
///     (&passes, &views),
///     offscreen.as_ref().map(|work| work as &dyn FrameOperations),
///     &mut [
///         BorrowedFeature { declaration, prepared: &mut stock },
///         BorrowedFeature { declaration: caller_declaration, prepared: &mut caller },
///     ],
///     &(uniform, None),
/// )?;
/// ```
pub struct StockSceneFeature<'a> {
    feature: SceneFeature<'a>,
    declaration: FeatureDeclaration,
    passes: Vec<PassDeclaration>,
    views: Vec<AttachmentView>,
    offscreen: Option<worldrunner_render_program::offscreen::AdmittedOffscreen>,
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
            operations: &[],
        }
    }

    /// Takes the admitted offscreen work retained during preparation, at most once.
    /// Keep it alive and pass its `FrameOperations` reference as the head of
    /// [`FrameRenderer::encode_frame`]; it is not included in [`Self::plan`].
    /// Retaining or taking this work does not encode it. A `None` head still records the
    /// stock draws without refreshing their offscreen inputs. Forward any returned work
    /// on the preparation encoder before the stock passes, or abandon the frame.
    #[must_use]
    pub fn take_offscreen_operations(
        &mut self,
    ) -> Option<worldrunner_render_program::offscreen::AdmittedOffscreen> {
        self.offscreen.take()
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
    pipelines: crate::renderer::ScenePipelines,
    color: &'b AttachmentView,
    depth: &'b AttachmentView,
    frame: crate::frame::FrameUniform,
    screen: Option<crate::scene_metadata::Geometry>,
    velocity: Option<AttachmentView>,
    programs: Option<worldrunner_render_program::runtime::AdmittedPrograms>,
    offscreen: Option<worldrunner_render_program::offscreen::AdmittedOffscreen>,
    depth_mode: crate::renderer::depth::DepthMode,
}

fn declare_scene_passes(
    renderer: &FrameRenderer,
    device: &wgpu::Device,
    scene: &SceneDraw<'_, '_>,
) -> Result<super::stock_plan::Base, FrameError> {
    super::unshadowed(
        renderer,
        device,
        &super::SceneInputs {
            gpu: &*scene.gpu,
            pipelines: &scene.pipelines,
            color: scene.color,
            depth: scene.depth,
            velocity: scene.velocity.as_ref(),
            depth_mode: scene.depth_mode,
        },
    )
}

/// Prepares a resident unshadowed scene as a feature that callers compose with their own.
/// Performs depth and effect validation, retains the player's admitted offscreen work, stages
/// dynamic and terrain uploads, and prepares the stock plan.
/// Checked attachments retain their selected mip, array layer and view format.
///
/// Take the returned feature's offscreen operations and forward them to
/// [`FrameRenderer::encode_frame`]. Preparation retains that work without encoding it;
/// the frame executor runs it ahead of the stock passes.
///
/// The submission and abandonment rules of [`FrameRenderer`] apply. Dynamic uploads are staged
/// into `encoder` now, so the feature must be encoded in that encoder's frame or the frame dropped.
///
/// # Errors
/// As [`FrameEncoding::encode_scene`](super::FrameEncoding::encode_scene).
pub fn prepare_stock_scene<'a>(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (
        &'a mut crate::draw::SceneGpu,
        &crate::renderer::ScenePipelines,
    ),
    attachments: ColorDepth<'_>,
    frame: crate::frame::FrameUniform,
) -> Result<StockSceneFeature<'a>, FrameError> {
    let (gpu, pipelines) = scene;
    let scene = SceneRefs { gpu, pipelines };
    let draw = prepare_scene_draw(renderer, device, encoder, scene, attachments, &frame)?;
    finish_stock_scene(renderer, device, encoder, draw)
}

/// The scene's GPU state, mutable, and the pipelines its draws select from.
struct SceneRefs<'a, 'p> {
    gpu: &'a mut crate::draw::SceneGpu,
    pipelines: &'p crate::renderer::ScenePipelines,
}

/// The scene colour and depth attachments of one frame.
#[derive(Clone, Copy)]
pub struct ColorDepth<'a> {
    pub color: &'a AttachmentView,
    pub depth: &'a AttachmentView,
}

fn prepare_scene_draw<'a, 'b>(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: SceneRefs<'a, '_>,
    attachments: ColorDepth<'b>,
    frame: &crate::frame::FrameUniform,
) -> Result<SceneDraw<'a, 'b>, FrameError> {
    let SceneRefs { gpu, pipelines } = scene;
    let ColorDepth { color, depth } = attachments;
    renderer.validate_taa_depth(depth)?;
    validate_effects(renderer, gpu, pipelines)?;
    if let Some(screen) = &mut renderer.ssao {
        screen.receiver = None;
    }
    renderer.prepare_scene_inputs(device, gpu);
    let depth_mode = renderer.forward_depth(gpu, depth);
    let selected = renderer.color_pipelines(pipelines, depth_mode);
    let pipelines = &selected;
    let writes_velocity = renderer.adopt_scene_velocity(gpu, pipelines);
    let velocity = renderer.velocity_attachment(writes_velocity)?;
    let (programs, offscreen) =
        encode_player_work(gpu, encoder, renderer, (pipelines, velocity.is_some()))?;
    gpu.prepare_dynamic(encoder, pipelines)?;
    let frame = renderer.prepare_uniform(*frame)?;
    gpu.prepare_terrain_geometry(device, encoder, &frame, color.size())?;
    renderer.publish_game_terrain(gpu, &frame, color.size());
    gpu.prepare_sky(device, encoder, &frame)?;
    gpu.prepare_terrain_pages(device, encoder, &frame, color.size())?;
    let screen = renderer.geometry_for_depth(depth_mode);
    prepare_terrain(gpu, &frame, pipelines, screen.as_ref())?;
    Ok(SceneDraw {
        gpu,
        pipelines: selected,
        color,
        depth,
        frame,
        screen,
        velocity,
        programs,
        offscreen,
        depth_mode,
    })
}

fn finish_stock_scene<'a>(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &wgpu::CommandEncoder,
    mut draw: SceneDraw<'a, '_>,
) -> Result<StockSceneFeature<'a>, FrameError> {
    let mut plan = declare_scene_passes(renderer, device, &draw)?;
    let contour = renderer.plan_contours(
        (device, encoder),
        (draw.gpu, &draw.pipelines),
        &draw.frame,
        (&mut plan.passes, &mut plan.views, &mut plan.declaration),
    )?;
    let offscreen = draw.offscreen.take();
    Ok(StockSceneFeature {
        feature: draw.into_feature(contour),
        declaration: plan.declaration,
        passes: plan.passes,
        views: plan.views,
        offscreen,
    })
}

impl<'a> SceneDraw<'a, '_> {
    fn into_feature(self, contour: Option<crate::draw::ContourDraw>) -> SceneFeature<'a> {
        SceneFeature {
            publications: crate::draw::DrawPublications::default(),
            screen: self.screen,
            programs: self.programs,
            contour,
            gpu: self.gpu,
            pipelines: Cow::Owned(self.pipelines),
            frame: self.frame,
            opaque_drawn: std::cell::Cell::new(false),
        }
    }
}
