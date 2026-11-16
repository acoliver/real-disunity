//! Preparation and scheduling of a fitted shadowed frame.
use super::{
    DirectionalShadowSettings, FrameUniform, GpuContext, SceneView, ShadowError, ShadowFeature,
    ShadowedScene, cascades::CascadeViews, resources::ShadowMap, schedule, sdsm,
};
use crate::{compositor::FrameRenderer, draw::SceneGpu, renderer::ScenePipelines};
use worldrunner_render_plan::SceneEffects;
use worldrunner_render_post::{AttachmentView, BorrowedFeature};

struct ShadowEncoding<'a> {
    gpu: &'a mut SceneGpu,
    pipelines: &'a ScenePipelines,
    effects: &'a SceneEffects,
    object_caster: &'a crate::object::program::Context,
    terrain_caster: &'a super::terrain_caster::TerrainCaster,
    depth_frames: &'a [(wgpu::Buffer, wgpu::BindGroup)],
    settings: DirectionalShadowSettings,
    shadows_enabled: bool,
}

struct PreparedShadowFrame {
    selected: ScenePipelines,
    frame: FrameUniform,
    screen: Option<crate::scene_metadata::Geometry>,
    cascades: CascadeViews,
}

impl ShadowedScene {
    pub(super) fn encode_fitted(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        input: (
            Option<&mut FrameRenderer>,
            Option<(sdsm::DepthRange, sdsm::RangeFit)>,
        ),
    ) -> Result<(), ShadowError> {
        self.update_caster_pipelines(context);
        let mut replacement = self.replacement_map(context)?;
        if let Some(map) = replacement.as_ref() {
            map.retain(encoder)?;
        } else {
            self.map.retain(encoder)?;
        }
        let mut next_refresh = self.refresh_for_frame(replacement.is_some());
        let (scheduler, fitting) = input;
        let scheduler = scheduler.unwrap_or(&mut self.scheduler);
        let mut encoding = ShadowEncoding {
            gpu: &mut self.gpu,
            pipelines: &self.pipelines,
            effects: &self.effects,
            object_caster: &self.object_caster,
            terrain_caster: &self.terrain_caster,
            depth_frames: &self.depth_frames,
            settings: self.settings,
            shadows_enabled: self.shadows_enabled,
        };
        let mut prepared =
            encoding.prepare(context, (view, fitting), attachments, encoder, scheduler)?;
        let refresh = next_refresh.prepare(
            encoding.gpu,
            &mut prepared.cascades,
            self.settings,
            prepared.frame.shadow_direction,
            attachments.0.size(),
            self.shadows_enabled,
        );
        let map = replacement.as_mut().unwrap_or(&mut self.map);
        map.select_frame_slot(self.frame_slot);
        encoding.encode(
            context,
            encoder,
            scheduler,
            attachments,
            (map, prepared, refresh),
        )?;
        if let Some(map) = replacement {
            self.map = map;
        }
        next_refresh.publish(encoder, &self.refresh)?;
        Ok(())
    }
}

impl ShadowEncoding<'_> {
    fn prepare(
        &mut self,
        context: &GpuContext,
        (view, fitting): (&SceneView, Option<(sdsm::DepthRange, sdsm::RangeFit)>),
        (color, depth): (&AttachmentView, &AttachmentView),
        encoder: &mut wgpu::CommandEncoder,
        scheduler: &mut FrameRenderer,
    ) -> Result<PreparedShadowFrame, ShadowError> {
        let object_casters = self.shadows_enabled.then_some(self.object_caster);
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
        let screen = scheduler.geometry_for_depth(depth_mode);
        ShadowedScene::prepare_terrain_passes(
            (self.gpu, self.terrain_caster, object_casters),
            (screen.as_ref(), &selected, &frame),
        )?;
        let cascades = ShadowedScene::prepare_cascades(
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
        Ok(PreparedShadowFrame {
            selected,
            frame,
            screen,
            cascades,
        })
    }

    fn encode(
        &mut self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        scheduler: &mut FrameRenderer,
        (color, depth): (&AttachmentView, &AttachmentView),
        (map, prepared, refresh): (&ShadowMap, PreparedShadowFrame, [bool; 4]),
    ) -> Result<(), ShadowError> {
        let PreparedShadowFrame {
            selected,
            frame,
            screen,
            cascades,
        } = prepared;
        let shadow_groups = map.receiver_groups(
            scheduler,
            context.device(),
            selected.card_reject().then_some(depth),
        );
        let (mut schedule, contour) = schedule::ShadowSchedule::prepared(
            scheduler,
            (context.device(), encoder),
            (self.gpu, &selected),
            &frame,
            (color, depth),
            (&map.depth, refresh),
        )?;
        let mut feature = ShadowFeature {
            contour,
            deferred: schedule.deferred.take(),
            renders: scheduler
                .gbuffer
                .as_ref()
                .is_some_and(crate::gbuffer::Gbuffer::renders),
            publications: crate::draw::DrawPublications::default(),
            screen,
            gpu: self.gpu,
            pipelines: &selected,
            object_caster: self.object_caster,
            terrain_caster: self.terrain_caster,
            depth_frames: self.depth_frames,
            shadow_groups: &shadow_groups,
            cascades,
            refresh,
            frame,
            shadow_uniform: &map.uniform,
            settings: self.settings,
            sampling: self.effects.shadows().sampling(),
        };
        feature.encode(context, encoder, scheduler, schedule, &map.group)
    }
}

impl ShadowFeature<'_> {
    fn encode(
        &mut self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        scheduler: &mut FrameRenderer,
        schedule: schedule::ShadowSchedule,
        shadow_group: &wgpu::BindGroup,
    ) -> Result<(), ShadowError> {
        let frame = self.frame;
        scheduler.encode_frame(
            context.device(),
            encoder,
            (&schedule.passes, &schedule.views),
            &mut [BorrowedFeature {
                declaration: schedule.declaration,
                prepared: self,
            }],
            (frame, Some(shadow_group)),
        )?;
        Ok(())
    }
}

/// The uniform a shadow caster pass draws with: the light's own matrix and nothing of the
/// viewer's sub-pixel jitter, which would make cascade edges move with the camera. The material
/// mip bias is a screen-space policy for the reconstructed view, so a light-view raster uses 0.
pub(crate) fn light_frame(frame: &FrameUniform, matrix: glam::Mat4) -> FrameUniform {
    FrameUniform {
        // Directional light fits use right-handed views independently of the viewer.
        view_convention: worldrunner_camera::ViewConvention::RIGHT_HANDED,
        view_projection: matrix,
        temporal: None,
        material_mip_bias: 0.0,
        ..*frame
    }
}
