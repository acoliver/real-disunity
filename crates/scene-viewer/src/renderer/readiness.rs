use super::{Presentation, Renderer, Resident, ViewerError, required_features};
use crate::RetainedHost;
use std::sync::mpsc::TryRecvError;

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
    fn collect(&mut self, completed: u64) {
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
            objects: super::retained::Objects::default(),
            retired: std::collections::VecDeque::new(),
            shadows,
            profile,
        }
    }
}

#[cfg(test)]
mod tests;
