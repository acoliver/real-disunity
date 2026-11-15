use super::offscreen::encode_player_work;
use super::{FrameRenderer, attachment, attachment_resource};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, FrameError, PassDeclaration, PassSlot,
    PreparedFeature,
};

pub(crate) struct SceneFeature<'a> {
    pub(crate) publications: crate::draw::DrawPublications,
    pub gpu: &'a mut crate::draw::SceneGpu,
    pub pipelines: &'a crate::renderer::ScenePipelines,
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
                self.gpu.draw_forward_remainder(pass, self.pipelines),
            );
            return;
        }
        self.publications
            .record_color(self.gpu, self.gpu.draw_color(pass, self.pipelines, None));
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
                    let ao = colour_pipelines(self.pipelines, Some(screen)).has_ao_receiver();
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
            return self.pipelines;
        };
        if let Some((pipelines, group)) = &screen.forward {
            pass.set_bind_group(3, group, &[]);
            return pipelines;
        }
        pass.set_bind_group(3, &screen.ambient, &[]);
        self.pipelines
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

struct SceneDraw<'a> {
    gpu: &'a mut crate::draw::SceneGpu,
    pipelines: &'a crate::renderer::ScenePipelines,
    color: &'a AttachmentView,
    depth: &'a AttachmentView,
    frame: crate::frame::FrameUniform,
    screen: Option<crate::scene_metadata::Geometry>,
    velocity: Option<AttachmentView>,
    programs: Option<worldrunner_render_program::runtime::AdmittedPrograms>,
    offscreen: Option<worldrunner_render_program::offscreen::AdmittedOffscreen>,
    depth_mode: crate::renderer::depth::DepthMode,
}

fn encode_scene_passes(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: SceneDraw<'_>,
) -> Result<(), FrameError> {
    let plan = super::unshadowed(
        renderer,
        device,
        &super::SceneInputs {
            gpu: &*scene.gpu,
            pipelines: scene.pipelines,
            color: scene.color,
            depth: scene.depth,
            velocity: scene.velocity.as_ref(),
            depth_mode: scene.depth_mode,
        },
    )?;
    let SceneDraw {
        gpu,
        pipelines,
        frame,
        screen,
        programs,
        offscreen,
        depth_mode,
        ..
    } = scene;
    let super::stock_plan::Base {
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
            &depth_texture,
            depth_mode == crate::renderer::depth::DepthMode::Reuse,
        ),
        &frame,
        &mut declaration,
    )?;
    let mut feature = SceneFeature {
        publications: renderer.draw_publications(),
        screen,
        programs,
        gpu,
        pipelines,
        frame,
        deferred,
        renders: renderer
            .gbuffer
            .as_ref()
            .is_some_and(crate::gbuffer::Gbuffer::renders),
        opaque_drawn: std::cell::Cell::new(false),
    };
    renderer.encode_frame_with(
        device,
        encoder,
        (&passes, &views),
        offscreen
            .as_ref()
            .map(|work| work as &dyn worldrunner_render_post::FrameOperations),
        &mut [BorrowedFeature {
            declaration,
            prepared: &mut feature,
        }],
        &(frame, None),
    )
}

/// Encodes a resident unshadowed scene through the feature boundary.
/// Checked attachments retain their selected mip, array layer and view format.
/// The submission and abandonment rules of [`FrameRenderer`] apply.
pub fn encode_scene(
    renderer: &mut FrameRenderer,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    scene: (&mut crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
    attachments: (&AttachmentView, &AttachmentView),
    frame: crate::frame::FrameUniform,
) -> Result<(), FrameError> {
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
    let writes_velocity = renderer.draws_velocity(pipelines);
    renderer.match_producer_velocity(device, writes_velocity);
    gpu.track_previous_transforms(writes_velocity);
    let velocity = renderer.velocity_attachment(writes_velocity)?;
    let (programs, offscreen) =
        encode_player_work(gpu, encoder, renderer, (pipelines, velocity.is_some()))?;
    gpu.prepare_dynamic(encoder, pipelines)?;
    let frame = renderer.prepare_uniform(frame)?;
    let (color, depth) = attachments;
    gpu.prepare_terrain_geometry(device, encoder, &frame, color.size())?;
    gpu.prepare_sky(device, encoder, &frame)?;
    gpu.prepare_terrain_pages(device, encoder, &frame, color.size())?;
    let screen = renderer.geometry_for_depth(depth_mode);
    prepare_terrain(gpu, &frame, pipelines, screen.as_ref())?;
    encode_scene_passes(
        renderer,
        device,
        encoder,
        SceneDraw {
            gpu,
            pipelines,
            color,
            depth,
            frame,
            screen,
            velocity,
            programs,
            offscreen,
            depth_mode,
        },
    )
}
