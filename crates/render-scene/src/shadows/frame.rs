use super::{
    AttachmentView, BorrowedFeature, DirectionalShadowSettings, FrameUniform, GpuContext,
    SceneEffects, SceneGpu, ScenePipelines, SceneView, ShadowError, ShadowMap, ShadowedScene,
    cascades, light_frame, refresh, resources, schedule, sdsm, terrain_caster,
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
    schedule: schedule::ShadowSchedule,
    deferred: Option<crate::gbuffer::lighting::LightingInputs>,
    map: &'a ShadowMap,
}

impl ShadowedScene {
    pub(super) fn encode_fitted(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        input: (
            Option<&mut crate::compositor::FrameRenderer>,
            Option<(sdsm::DepthRange, sdsm::RangeFit)>,
        ),
    ) -> Result<(), ShadowError> {
        let scheduler = input.0.as_deref().unwrap_or(&self.scheduler);
        scheduler.validate_opaque_depth(attachments.1)?;
        scheduler.validate_opaque_normal_bias(self.settings.normal_bias_meters)?;
        self.update_caster_pipelines(context);
        let mut replacement = self.replacement_map(context)?;
        let mut next_refresh = self.refresh_for_frame(replacement.is_some());
        let (scheduler, fitting) = input;
        let scheduler = scheduler.unwrap_or(&mut self.scheduler);
        let mut resources = ShadowFrameResources {
            gpu: &mut self.gpu,
            pipelines: &self.pipelines,
            object_caster: &self.object_caster,
            terrain_caster: &self.terrain_caster,
            depth_frames: &self.depth_frames,
            effects: &self.effects,
            settings: self.settings,
            shadows_enabled: self.shadows_enabled,
        };
        let scene =
            resources.prepare_scene_frame(context, view, attachments, encoder, scheduler)?;
        let shadow = resources.prepare_shadow_work(
            (context, view, encoder),
            &scene.frame,
            fitting,
            &mut next_refresh,
            attachments.0.size(),
        )?;
        let map = replacement.as_mut().unwrap_or(&mut self.map);
        map.select_frame_slot(self.frame_slot);
        let work = shadow.complete_shadow_schedule(
            context,
            attachments,
            scheduler,
            (&*resources.gpu, &scene),
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
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<SceneFrame, ShadowError> {
        let (color, depth) = attachments;
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
        scheduler.match_producer_velocity(context.device(), selected.writes_velocity());
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
        context_view_encoder: (&GpuContext, &SceneView, &mut wgpu::CommandEncoder),
        frame: &FrameUniform,
        fitting: Option<(sdsm::DepthRange, sdsm::RangeFit)>,
        next_refresh: &mut refresh::RefreshCache,
        size: [u32; 2],
    ) -> Result<ShadowWork, ShadowError> {
        let (context, view, encoder) = context_view_encoder;
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
        let ShadowScheduleWork {
            shadow,
            groups,
            schedule,
            deferred,
            map,
        } = work;
        let mut feature = ShadowFeature {
            publications: scheduler.draw_publications(),
            renders: scheduler
                .gbuffer
                .as_ref()
                .is_some_and(crate::gbuffer::Gbuffer::renders),
            screen: scene.screen,
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
            deferred,
        };
        scheduler.encode_frame(
            context.device(),
            encoder,
            (&schedule.passes, &schedule.views),
            &mut [BorrowedFeature {
                declaration: schedule.declaration,
                prepared: &mut feature,
            }],
            (scene.frame, Some(&map.group)),
        )?;
        Ok(())
    }
}

impl ShadowWork {
    fn complete_shadow_schedule<'a>(
        self,
        context: &GpuContext,
        attachments: (&AttachmentView, &AttachmentView),
        scheduler: &crate::compositor::FrameRenderer,
        scene: (&SceneGpu, &SceneFrame),
        map: &'a ShadowMap,
    ) -> Result<ShadowScheduleWork<'a>, ShadowError> {
        let ((color, depth), (gpu, scene)) = (attachments, scene);
        let card_depth = scene.selected.card_reject().then_some(depth);
        let groups = map.receiver_groups(scheduler, context.device(), card_depth);
        let mut schedule =
            schedule::ShadowSchedule::new(&map.depth, color, depth, &scene.selected, self.refresh)?;
        let deferred = schedule.complete(
            scheduler,
            (gpu, &scene.selected),
            (context.device(), color.size()),
            (scene.depth_mode, card_depth.is_some()),
            (depth.texture(), &scene.frame),
        )?;
        Ok(ShadowScheduleWork {
            shadow: self,
            groups,
            schedule,
            deferred,
            map,
        })
    }
}

struct ShadowFeature<'a> {
    publications: crate::draw::DrawPublications,
    renders: bool,
    screen: Option<crate::scene_metadata::Geometry>,
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
    deferred: Option<crate::gbuffer::lighting::LightingInputs>,
}

impl worldrunner_render_post::PreparedFeature for ShadowFeature<'_> {
    fn prepare(
        &mut self,
        _context: &mut worldrunner_render_post::PrepareContext<'_>,
    ) -> Result<(), worldrunner_render_post::FrameError> {
        Ok(())
    }

    fn commit(&mut self, context: &mut worldrunner_render_post::PrepareContext<'_>) {
        self.gpu.reset_pass_statistics();
        self.publications.install(
            context,
            crate::draw::PassInstallation::Installed,
            self.screen.is_some(),
            self.gpu.ssao,
        );
        self.gpu.prepare_instances(context);
        self.gpu.commit_frame(context, self.frame);
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
        if context.slot == worldrunner_render_post::PassSlot::Celestial {
            self.gpu.draw_celestial(pass);
            return;
        }
        #[cfg(test)]
        crate::scene_metadata::record_pass(context.slot);
        match context.slot {
            worldrunner_render_post::PassSlot::OpaqueGbuffer => self.draw_gbuffer(pass),
            worldrunner_render_post::PassSlot::DepthNormalPrepass if self.renders => {
                self.draw_gbuffer(pass);
            }
            worldrunner_render_post::PassSlot::DepthNormalPrepass => self.draw_prepass(pass),
            worldrunner_render_post::PassSlot::ShadowCasters(index) => {
                self.draw_casters(pass, index);
            }
            slot => self.draw_color(pass, slot),
        }
    }
}

impl ShadowFeature<'_> {
    fn draw_gbuffer(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.renders {
            self.publications.prepass();
        }
        let screen = self.screen.as_ref().expect("declared G-buffer producer");
        let (pipelines, group) = screen.gbuffer.as_ref().expect("G-buffer pipelines");
        let draw = self.gpu.draw_gbuffer(pass, pipelines, group);
        if self.renders {
            self.publications
                .record_gbuffer(draw, true, self.pipelines.has_ao_receiver());
        }
    }

    fn draw_prepass(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.publications.prepass();
        let screen = self.screen.as_ref().expect("declared prepass");
        self.publications.record(self.gpu.draw_prepass(
            pass,
            screen.prepass_pipelines(),
            &screen.group,
        ));
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

    fn draw_color(&self, pass: &mut wgpu::RenderPass<'_>, slot: worldrunner_render_post::PassSlot) {
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
        if slot == worldrunner_render_post::PassSlot::OpaqueLighting {
            let inputs = self.deferred.as_ref().expect("declared lighting pass");
            self.gpu
                .draw_deferred_lighting(pass, self.pipelines, inputs);
        } else if self.renders {
            self.publications.record_remainder(
                self.gpu,
                self.gpu.draw_forward_remainder(pass, self.pipelines),
            );
        } else {
            self.publications.record_color(
                self.gpu,
                self.gpu
                    .draw_shadowed(pass, self.pipelines, &self.shadow_groups.terrain),
            );
        }
    }
}
