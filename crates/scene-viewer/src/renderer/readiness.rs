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
        if !needs_pages {
            return Ok(false);
        }
        let ready = self.view_pages_ready(view)?;
        if ready && host.readiness.page_wait {
            host.readiness.page_wait = false;
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
        if host.readiness.page_wait || host.captures.holds_generation(host.ready_generation()) {
            return Ok(());
        }
        let Some(front) = host.readiness.queue.front_mut() else {
            return Ok(());
        };
        let scene = if let Some(receive) = &front.scene {
            match receive.try_recv() {
                Ok(result) => Some(result?),
                Err(TryRecvError::Empty) => return Ok(()),
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
            let scene = prepared.scene().clone();
            let serial = context.submission().submitted_serial();
            let mut next_resident = self.prepare_retained_scene(context, prepared)?;
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
    pub(super) fn collect(&mut self, completed: u64) {
        while self.retired.front().is_some_and(|old| old.0 <= completed) {
            self.retired.pop_front();
        }
    }

    fn prepare_retained_scene(
        &self,
        context: &super::GpuContext,
        prepared: worldrunner_render_scene::draw::PreparedScene,
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
        Resident::from_prepared(context, prepared, self.shadows, self.profile)
    }
}

pub(super) struct Streaming {
    pub captures: super::capture::Captures,
    pub objects: super::retained::Objects,
    pub retired: std::collections::VecDeque<(u64, Resident)>,
    pub shadows: Option<worldrunner_render_scene::shadows::DirectionalShadowSettings>,
    pub profile: crate::FrameProfile,
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
        }
    }
}

#[cfg(test)]
mod tests;
