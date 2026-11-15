//! Preparation and scheduling of a fitted shadowed frame.
use super::{
    FrameUniform, GpuContext, SceneView, ShadowError, ShadowFeature, ShadowedScene, schedule, sdsm,
};
use worldrunner_render_post::{AttachmentView, BorrowedFeature};

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
        self.update_caster_pipelines(context);
        let mut replacement = self.replacement_map(context)?;
        let mut next_refresh = self.refresh_for_frame(replacement.is_some());
        let ((scheduler, fitting), (color, depth)) = (input, attachments);
        let object_casters = self.shadows_enabled.then_some(&self.object_caster);
        let scheduler = scheduler.unwrap_or(&mut self.scheduler);
        Self::validate_scheduler(scheduler, &self.pipelines, &self.effects)?;
        scheduler.prepare_receiver_gradients(
            context.device(),
            &self.gpu,
            self.settings.normal_bias_meters,
        )?;
        scheduler.prepare_scene_inputs(context.device(), &self.gpu);
        let depth_mode = scheduler.forward_depth(&self.gpu, depth);
        let selected = scheduler
            .receiver_pipelines(&self.pipelines, depth_mode)
            .for_depth(depth_mode);
        self.gpu.prepare_dynamic(encoder, &selected)?;
        let frame = scheduler.prepare_uniform(FrameUniform::new(view, Some(&self.effects))?)?;
        self.gpu
            .prepare_frame_geometry(context.device(), encoder, &frame, color.size())?;
        let screen = scheduler.geometry_for_depth(depth_mode);
        Self::prepare_terrain_passes(
            (&self.gpu, &self.terrain_caster, object_casters),
            (screen.as_ref(), &selected, &frame),
        )?;
        let mut cascades = Self::prepare_cascades(
            (&self.gpu, &self.effects, self.settings),
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
            &self.gpu,
            &mut cascades,
            self.settings,
            frame.shadow_direction,
            color.size(),
            self.shadows_enabled,
        );
        let map = replacement.as_mut().unwrap_or(&mut self.map);
        map.select_frame_slot(self.frame_slot);
        let shadow_groups = scheduler.shadow_groups(
            context.device(),
            (
                &map.group,
                crate::scene_metadata::receiver::ShadowInputs {
                    uniform: &map.uniform,
                    depth: map.depth.receiver_view(),
                    sampler: &map.sampler,
                },
            ),
            selected.card_reject().then_some(depth),
        );
        let (schedule, contour) = schedule::ShadowSchedule::prepared(
            scheduler,
            (context.device(), encoder),
            (&self.gpu, &selected),
            &frame,
            (color, depth),
            (&map.depth, refresh),
        )?;
        let mut feature = ShadowFeature {
            contour,
            publications: crate::draw::DrawPublications::default(),
            screen,
            gpu: &mut self.gpu,
            pipelines: &selected,
            object_caster: &self.object_caster,
            terrain_caster: &self.terrain_caster,
            depth_frames: &self.depth_frames,
            shadow_groups: &shadow_groups,
            cascades,
            refresh,
            frame,
            shadow_uniform: &map.uniform,
            settings: self.settings,
            sampling: self.effects.shadows().sampling(),
        };
        scheduler.encode_frame(
            context.device(),
            encoder,
            (&schedule.passes, &schedule.views),
            &mut [BorrowedFeature {
                declaration: schedule.declaration,
                prepared: &mut feature,
            }],
            (frame, Some(&map.group)),
        )?;
        if let Some(map) = replacement {
            self.map = map;
        }
        next_refresh.publish(encoder, &self.refresh)?;
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
