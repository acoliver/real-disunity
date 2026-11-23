use crate::compositor::ColorDepth;

use super::{
    BorrowedFeature, DirectionalShadowSettings, FrameUniform, GpuContext, SceneEffects, SceneGpu,
    ScenePipelines, SceneView, ShadowError, ShadowMap, ShadowedScene, cascades, lent, light_frame,
    refresh, resources, sdsm, terrain_caster,
};

#[path = "compaction.rs"]
mod compaction;

struct ShadowFrameResources<'a> {
    gpu: &'a mut SceneGpu,
    pipelines: &'a ScenePipelines,
    object_caster: &'a crate::object::program::Context,
    terrain_caster: &'a terrain_caster::TerrainCaster,
    depth_frames: &'a [(wgpu::Buffer, wgpu::BindGroup)],
    effects: &'a SceneEffects,
    settings: DirectionalShadowSettings,
    shadows_enabled: bool,
    /// Player offscreen work the frame admitted, run before its first pass.
    offscreen: Option<worldrunner_render_program::offscreen::AdmittedOffscreen>,
}

/// The renderer a frame schedules through, and the depth range and fit to cascade by, when
/// the frame is depth-fitted. Empty depth evidence clears all maps and disables receiver
/// sampling; the ordinary frame retains its original schedule.
pub(super) struct FitInput<'a> {
    pub(super) scheduler: &'a mut crate::compositor::FrameRenderer,
    pub(super) fitting: Option<(sdsm::DepthRange, sdsm::RangeFit)>,
}

/// What the cascade preparation encodes into and looks through.
struct ShadowWorkContext<'a> {
    context: &'a GpuContext,
    view: &'a SceneView,
    encoder: &'a mut wgpu::CommandEncoder,
}

/// The device context and the encoder the schedule's contour planning records against.
#[derive(Clone, Copy)]
struct ContextEncoder<'a> {
    context: &'a GpuContext,
    encoder: &'a wgpu::CommandEncoder,
}

/// The scene state a shadow schedule is completed from.
#[derive(Clone, Copy)]
struct ScheduledScene<'a> {
    gpu: &'a SceneGpu,
    frame: &'a SceneFrame,
}

struct SceneFrame {
    depth_mode: crate::renderer::depth::DepthMode,
    selected: ScenePipelines,
    frame: FrameUniform,
    screen: Option<crate::scene_metadata::Geometry>,
}

struct ShadowWork {
    cascades: cascades::CascadeViews,
    refresh: [bool; 4],
}

struct ShadowScheduleWork<'a> {
    shadow: ShadowWork,
    groups: crate::scene_metadata::receiver::SetTwo,
    schedule: crate::compositor::ShadowSchedule,
    contour: Option<crate::draw::ContourDraw>,
    map: &'a ShadowMap,
}

impl ShadowedScene {
    pub(super) fn encode_fitted(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: ColorDepth<'_>,
        encoder: &mut wgpu::CommandEncoder,
        input: FitInput<'_>,
    ) -> Result<(), ShadowError> {
        self.refuse_programs()?;
        let FitInput { scheduler, fitting } = input;
        let offscreen = crate::compositor::offscreen::take_offscreen(&mut self.gpu, encoder)?;
        self.update_caster_pipelines(context);
        let mut replacement = self.replacement_map(context)?;
        let mut next_refresh = self.refresh_for_frame(replacement.is_some());
        let mut resources = ShadowFrameResources {
            gpu: &mut self.gpu,
            pipelines: &self.pipelines,
            object_caster: &self.object_caster,
            terrain_caster: &self.terrain_caster,
            depth_frames: &self.depth_frames,
            effects: &self.effects,
            settings: self.settings,
            shadows_enabled: self.shadows_enabled,
            offscreen,
        };
        let scene =
            resources.prepare_scene_frame(context, view, attachments, encoder, scheduler)?;
        let shadow = resources.prepare_shadow_work(
            ShadowWorkContext {
                context,
                view,
                encoder,
            },
            &scene.frame,
            fitting,
            &mut next_refresh,
            attachments.color.size(),
        )?;
        let map = replacement.as_mut().unwrap_or(&mut self.map);
        map.select_frame_slot(self.frame_slot);
        map.retain(encoder)?;
        let work = shadow.complete_shadow_schedule(
            ContextEncoder { context, encoder },
            attachments,
            scheduler,
            ScheduledScene {
                gpu: &*resources.gpu,
                frame: &scene,
            },
            map,
        )?;
        resources.encode_schedule(context, encoder, scheduler, scene, work)?;
        if let Some(map) = replacement {
            self.map = map;
        }
        next_refresh.publish(encoder, &self.refresh)?;
        Ok(())
    }
}

impl ShadowFrameResources<'_> {
    fn prepare_scene_frame(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: ColorDepth<'_>,
        encoder: &mut wgpu::CommandEncoder,
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<SceneFrame, ShadowError> {
        let ColorDepth { color, depth } = attachments;
        ShadowedScene::validate_scheduler(scheduler, self.pipelines, self.effects)?;
        scheduler.prepare_receiver_gradients(
            context.device(),
            self.gpu,
            self.settings.normal_bias_meters,
        )?;
        scheduler.prepare_scene_inputs(context.device(), self.gpu);
        let depth_mode = scheduler.forward_depth(self.gpu, depth);
        let selected = scheduler
            .receiver_pipelines(self.pipelines, depth_mode)
            .for_depth(depth_mode);
        self.gpu.prepare_dynamic(encoder, &selected)?;
        let frame = scheduler.prepare_uniform(FrameUniform::new(view, Some(self.effects))?)?;
        self.gpu
            .prepare_frame_geometry(context.device(), encoder, &frame, color.size())?;
        scheduler.publish_game_terrain(self.gpu, &frame, color.size());
        self.gpu
            .track_previous_transforms(selected.writes_velocity());
        let screen = scheduler.geometry_for_depth(depth_mode);
        ShadowedScene::prepare_terrain_passes(
            (
                self.gpu,
                self.terrain_caster,
                self.shadows_enabled.then_some(self.object_caster),
            ),
            (screen.as_ref(), &selected, &frame),
        )?;
        Ok(SceneFrame {
            depth_mode,
            selected,
            frame,
            screen,
        })
    }

    fn prepare_shadow_work(
        &self,
        work_context: ShadowWorkContext<'_>,
        frame: &FrameUniform,
        fitting: Option<(sdsm::DepthRange, sdsm::RangeFit)>,
        next_refresh: &mut refresh::RefreshCache,
        size: [u32; 2],
    ) -> Result<ShadowWork, ShadowError> {
        let ShadowWorkContext {
            context,
            view,
            encoder,
        } = work_context;
        let mut cascades = ShadowedScene::prepare_cascades(
            (self.gpu, self.effects, self.settings),
            view,
            frame.shadow_direction,
            fitting,
        )?;
        self.gpu.prepare_caster_terrain(
            context.device(),
            encoder,
            &cascades.matrices,
            self.settings.resolution,
        )?;
        let refresh = next_refresh.prepare(
            self.gpu,
            &mut cascades,
            self.settings,
            frame.shadow_direction,
            size,
            self.shadows_enabled,
        );
        Ok(ShadowWork { cascades, refresh })
    }

    fn encode_schedule(
        &mut self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        scheduler: &mut crate::compositor::FrameRenderer,
        scene: SceneFrame,
        work: ShadowScheduleWork<'_>,
    ) -> Result<(), ShadowError> {
        let offscreen = self.offscreen.take();
        let ShadowScheduleWork {
            shadow,
            groups,
            schedule,
            contour,
            map,
        } = work;
        scheduler.publish_game_shadows(|| {
            lent::lend(
                map,
                &shadow.cascades,
                self.settings,
                self.effects.shadows().sampling(),
            )
        });
        let mut feature = ShadowFeature {
            publications: crate::draw::DrawPublications::default(),
            screen: scene.screen,
            contour,
            gpu: self.gpu,
            pipelines: &scene.selected,
            object_caster: self.object_caster,
            terrain_caster: self.terrain_caster,
            depth_frames: self.depth_frames,
            shadow_groups: &groups,
            cascades: shadow.cascades,
            refresh: shadow.refresh,
            frame: scene.frame,
            shadow_uniform: &map.uniform,
            settings: self.settings,
            sampling: self.effects.shadows().sampling(),
        };
        scheduler.encode_frame(
            context.device(),
            encoder,
            (&schedule.passes, &schedule.views),
            offscreen
                .as_ref()
                .map(|work| work as &dyn worldrunner_render_post::FrameOperations),
            &mut [BorrowedFeature {
                declaration: schedule.declaration,
                prepared: &mut feature,
            }],
            &(scene.frame, Some(&map.group)),
        )?;
        Ok(())
    }
}

impl ShadowWork {
    fn complete_shadow_schedule<'a>(
        self,
        context: ContextEncoder<'_>,
        attachments: ColorDepth<'_>,
        scheduler: &mut crate::compositor::FrameRenderer,
        scene: ScheduledScene<'_>,
        map: &'a ShadowMap,
    ) -> Result<ShadowScheduleWork<'a>, ShadowError> {
        let ContextEncoder { context, encoder } = context;
        let ColorDepth { color, depth } = attachments;
        let ScheduledScene { gpu, frame: scene } = scene;
        let card_depth = scene.selected.card_reject().then_some(depth);
        let groups = map.receiver_groups(scheduler, context.device(), card_depth);
        let mut schedule = crate::compositor::ShadowSchedule::new(
            &map.depth,
            color,
            depth,
            &scene.selected,
            self.refresh,
        )?;
        schedule.complete(
            scheduler,
            (gpu, &scene.selected),
            (context.device(), color.size()),
            (scene.depth_mode, card_depth.is_some()),
        )?;
        let contour = scheduler.plan_contours(
            (context.device(), encoder),
            (gpu, &scene.selected),
            &scene.frame,
            (
                &mut schedule.passes,
                &mut schedule.views,
                &mut schedule.declaration,
            ),
        )?;
        Ok(ShadowScheduleWork {
            shadow: self,
            groups,
            schedule,
            contour,
            map,
        })
    }
}

struct ShadowFeature<'a> {
    publications: crate::draw::DrawPublications,
    screen: Option<crate::scene_metadata::Geometry>,
    contour: Option<crate::draw::ContourDraw>,
    gpu: &'a mut SceneGpu,
    pipelines: &'a ScenePipelines,
    object_caster: &'a crate::object::program::Context,
    terrain_caster: &'a terrain_caster::TerrainCaster,
    depth_frames: &'a [(wgpu::Buffer, wgpu::BindGroup)],
    shadow_groups: &'a crate::scene_metadata::receiver::SetTwo,
    cascades: cascades::CascadeViews,
    refresh: [bool; 4],
    frame: FrameUniform,
    shadow_uniform: &'a wgpu::Buffer,
    settings: DirectionalShadowSettings,
    sampling: worldrunner_render_plan::SceneShadowSampling,
}

impl worldrunner_render_post::PreparedFeature for ShadowFeature<'_> {
    fn prepare(
        &mut self,
        _context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        Ok(())
    }

    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.gpu.reset_pass_statistics(context.completed_in_prefix(
            worldrunner_render_post::PassEntry::new(
                worldrunner_render_post::PassSlot::DepthNormalPrepass,
            ),
        ));
        self.publications.install(
            context,
            crate::draw::PassInstallation::Installed,
            self.screen.is_some(),
            self.gpu.ssao,
        );
        self.gpu.prepare_instances(context);
        self.gpu.commit_frame(context, self.frame);
        self.select_views(context);
        self.gpu.prepare_view_compaction(context, None, &self.frame);
        self.prepare_cascade_compaction(context);
        for (index, &matrix) in self.cascades.matrices.iter().enumerate() {
            let light_frame = light_frame(&self.frame, matrix);
            context.upload_buffer(&self.depth_frames[index].0, 0, &light_frame.to_bytes());
        }
        context.upload_buffer(
            self.shadow_uniform,
            0,
            &resources::uniform_bytes_with_sampling(&self.cascades, self.settings, self.sampling),
        );
    }

    fn finish_draws(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.publications.publish(context, self.gpu);
    }

    fn draw(
        &self,
        context: &worldrunner_render_post::DrawContext,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        if context.slot == worldrunner_render_post::PassSlot::ContourIds {
            self.gpu
                .draw_contours(pass, self.contour.as_ref().expect("declared contour pass"));
            return;
        }
        if context.slot == worldrunner_render_post::PassSlot::Celestial {
            self.gpu.draw_celestial(pass);
            return;
        }
        #[cfg(test)]
        crate::scene_metadata::record_pass(context.slot);
        match context.slot {
            worldrunner_render_post::PassSlot::DepthNormalPrepass => self.draw_prepass(pass),
            worldrunner_render_post::PassSlot::ShadowCasters(index) => {
                self.draw_casters(pass, index);
            }
            _ => self.draw_color(pass),
        }
    }
}

impl ShadowFeature<'_> {
    fn draw_prepass(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.publications.prepass();
        let screen = self.screen.as_ref().expect("declared prepass");
        self.publications.record_prepass(
            self.gpu,
            self.gpu
                .draw_prepass(pass, screen.prepass_pipelines(), &screen.group),
        );
    }

    fn draw_casters(&self, pass: &mut wgpu::RenderPass<'_>, index: u32) {
        if self.cascades.no_receivers {
            return;
        }
        let matrix = self.cascades.matrices[index as usize];
        let group = &self.depth_frames[index as usize].1;
        self.publications.record(self.gpu.draw_casters_prepared(
            pass,
            group,
            self.object_caster,
            (index, matrix),
        ));
        let caster = self
            .terrain_caster
            .pipeline()
            .expect("the terrain caster is prepared before its passes");
        let terrain = self
            .gpu
            .draw_terrain_depth(pass, (&caster, group), (index, matrix));
        self.gpu.pass_counters.shadows[index as usize].add_recorded(terrain);
    }

    fn draw_color(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(screen) = &self.screen {
            pass.set_bind_group(
                3,
                screen
                    .forward
                    .as_ref()
                    .map_or(&screen.ambient, |(_, group)| group),
                &[],
            );
        }
        pass.set_bind_group(2, &self.shadow_groups.object, &[]);
        self.publications.record_color(
            self.gpu,
            self.gpu
                .draw_shadowed(pass, self.pipelines, &self.shadow_groups.terrain),
        );
    }
}
