use super::{Presentation, Renderer, Resident, ViewerError, required_features};
use crate::RetainedHost;
use std::sync::mpsc::TryRecvError;

impl Renderer {
    pub(crate) fn view_pages_ready(
        &mut self,
        view: &worldrunner_render_scene::SceneView,
    ) -> Result<bool, ViewerError> {
        let color = self.presentation.targets.color.texture();
        let viewport = [color.width(), color.height()];
        let frame = worldrunner_render_scene::frame::FrameUniform::new(view, None)?;
        let gpu = self.resident.scene_mut();
        let request = gpu
            .terrain_residency_request(&frame, viewport)
            .map_err(|error| ViewerError::Gpu(error.to_string()))?
            .expect("validated page dependency requires paging");
        gpu.terrain_residency_ready(&request)
            .map_err(|error| ViewerError::Gpu(error.to_string()))
    }

    fn view_geometry_ready(
        &mut self,
        view: &worldrunner_render_scene::SceneView,
    ) -> Result<bool, ViewerError> {
        let color = self.presentation.targets.color.texture();
        let viewport = [color.width(), color.height()];
        let frame = worldrunner_render_scene::frame::FrameUniform::new(view, None)?;
        let gpu = self.resident.scene_mut();
        let Some(request) = gpu.terrain_geometry_request(&frame, viewport) else {
            return Ok(true);
        };
        gpu.terrain_geometry_ready(&request)
            .map_err(|error| ViewerError::Gpu(error.to_string()))
    }

    pub(super) fn retained_pages_ready(
        &mut self,
        host: &mut RetainedHost,
        view: &worldrunner_render_scene::SceneView,
    ) -> Result<bool, ViewerError> {
        let needs_pages = host.readiness.page_wait
            || host
                .captures
                .waiting
                .iter()
                .any(|r| r.1 == host.readiness.published && r.2);
        let ready = needs_pages && self.view_pages_ready(view)?;
        let needs_geometry =
            host.readiness.geometry_wait
                || host.captures.waiting.iter().any(|r| {
                    r.1 == host.readiness.published && host.captures.geometry.contains(&r.0)
                });
        host.captures.geometry_ready = !needs_geometry || self.view_geometry_ready(view)?;
        if ready {
            host.readiness.page_wait = false;
        }
        if host.captures.geometry_ready {
            host.readiness.geometry_wait = false;
        }
        if !host.readiness.page_wait && !host.readiness.geometry_wait {
            host.readiness.ready = host.readiness.published;
        }
        Ok(ready)
    }
}

impl Renderer {
    pub(super) fn publish_retained(&mut self, host: &mut RetainedHost) -> Result<(), ViewerError> {
        self.streaming.publish(
            &self.context,
            &mut self.resident,
            &mut self.presentation,
            host,
        )
    }
}

impl Streaming {
    pub(super) fn publish(
        &mut self,
        context: &super::GpuContext,
        resident: &mut Resident,
        presentation: &mut Presentation,
        host: &mut RetainedHost,
    ) -> Result<(), ViewerError> {
        self.collect(context.submission().completed_serial());
        if host.readiness.page_wait
            || host.readiness.geometry_wait
            || host.captures.holds_generation(host.readiness.published)
        {
            return Ok(());
        }
        let Some(front) = host.readiness.queue.front_mut() else {
            return Ok(());
        };
        let serial = context.submission().submitted_serial();
        if !self.advance_preparation(context, resident, presentation, front)? {
            return Ok(());
        }
        if let Some((mut next_resident, scene, _reservation)) = self.preparing.take() {
            next_resident.scene_mut().finish_retained_terrain();
            let decals = next_resident.scene_mut().decal_placements();
            next_resident.scene_mut().set_decal_placements(
                front
                    .decal_ids
                    .iter()
                    .copied()
                    .zip(decals.into_iter().map(|(_, decal)| decal))
                    .collect(),
            )?;
            if let Some(effects) = scene.effects() {
                super::retained::configure_effects(
                    context,
                    &mut next_resident,
                    presentation,
                    effects,
                )?;
            }
            self.objects.handles = Some(
                front
                    .ids
                    .iter()
                    .copied()
                    .zip(next_resident.scene_mut().instance_handles())
                    .collect(),
            );
            let old_resident = std::mem::replace(resident, next_resident);
            self.retired.push_back((serial, old_resident));
        } else if let crate::RetainedUpdate::Decal(update) = &front.update {
            super::retained::apply_decal(resident.scene_mut(), update)?;
        } else {
            let effects = self
                .objects
                .apply_updates(resident.scene_mut(), [front.update.clone()])?;
            if let Some(effects) = effects {
                super::retained::configure_effects(context, resident, presentation, &effects)?;
            }
        }
        host.readiness.publish_front();
        Ok(())
    }
    fn advance_preparation(
        &mut self,
        context: &super::GpuContext,
        resident: &mut Resident,
        presentation: &mut Presentation,
        front: &crate::retained::readiness::Preparation,
    ) -> Result<bool, ViewerError> {
        let scene = if self.preparing.is_some() {
            None
        } else if let Some(receive) = &front.scene {
            match receive.try_recv() {
                Ok(result) => Some(result?),
                Err(TryRecvError::Empty) => return Ok(false),
                Err(TryRecvError::Disconnected) => {
                    return Err(ViewerError::Driver(
                        "scene preparation worker disconnected".into(),
                    ));
                }
            }
        } else {
            None
        };
        if let Some(prepared) = scene {
            let source = prepared.scene().clone();
            let reservation = presentation.prepare_scene(context, &source)?;
            let mut next = self.prepare_retained_scene(context, prepared, resident.scene_mut())?;
            next.scene_mut()
                .retain_resource_reservation(reservation.resident);
            self.preparing = Some((next, source, reservation.transient));
        }
        if let Some((next, _, _)) = &mut self.preparing {
            if next.scene_mut().retained_terrain_ready() {
                return Ok(true);
            }
            let mut ready = false;
            let prepared = context
                .submission()
                .prepare(context.device(), |encoder, _| {
                    let mut upload =
                        worldrunner_render_post::PrepareContext::new(context.device(), encoder)?;
                    ready = next.scene_mut().prepare_retained_terrain(&mut upload)?;
                    upload.finish();
                    Ok::<_, worldrunner_render_post::FrameError>(())
                })
                .map_err(|error| ViewerError::Gpu(error.to_string()))?;
            prepared
                .submit()
                .map_err(|error| ViewerError::Gpu(error.to_string()))?;
            if !ready {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(super) fn collect(&mut self, completed: u64) {
        while self.retired.front().is_some_and(|old| old.0 <= completed) {
            self.retired.pop_front();
        }
    }

    fn prepare_retained_scene(
        &self,
        context: &super::GpuContext,
        prepared: worldrunner_render_scene::draw::PreparedScene,
        previous: &worldrunner_render_scene::draw::SceneGpu,
    ) -> Result<Resident, ViewerError> {
        if !context
            .device()
            .features()
            .contains(required_features(prepared.scene()))
        {
            return Err(ViewerError::UnsupportedScene(
                "streamed textures require unavailable device features",
            ));
        }
        Resident::from_prepared_reusing(
            context,
            prepared,
            self.shadows,
            self.profile,
            Some(previous),
        )
    }
}

pub(super) struct Streaming {
    pub captures: super::capture::Captures,
    pub objects: super::retained::Objects,
    pub retired: std::collections::VecDeque<(u64, Resident)>,
    pub shadows: Option<worldrunner_render_scene::shadows::DirectionalShadowSettings>,
    pub profile: crate::FrameProfile,
    preparing: Option<(
        Resident,
        worldrunner_render_plan::RenderScene,
        worldrunner_render_gpu::configuration::AllocationReservation,
    )>,
}
impl Streaming {
    pub fn new(
        shadows: Option<worldrunner_render_scene::shadows::DirectionalShadowSettings>,
        profile: crate::FrameProfile,
    ) -> Self {
        Self {
            captures: super::capture::Captures::default(),
            objects: super::retained::Objects::default(),
            retired: std::collections::VecDeque::new(),
            shadows,
            profile,
            preparing: None,
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod preparation_tests;
