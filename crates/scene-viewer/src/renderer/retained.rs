use super::{Renderer, Resident};
use crate::{ObjectId, RetainedHost, RetainedUpdate, ViewerError};
use std::collections::BTreeMap;
use worldrunner_render_plan::SceneEffects;
use worldrunner_render_scene::draw::{InstanceHandle, SceneGpu};

#[derive(Default)]
pub(super) struct Objects {
    handles: Option<BTreeMap<ObjectId, InstanceHandle>>,
}

impl Objects {
    pub fn apply(
        &mut self,
        gpu: &mut SceneGpu,
        host: &mut RetainedHost,
    ) -> Result<Option<SceneEffects>, ViewerError> {
        let handles = self.handles.get_or_insert_with(|| {
            gpu.instance_handles()
                .enumerate()
                .map(|(index, handle)| (ObjectId(index as u64), handle))
                .collect()
        });
        let mut effects = None;
        for update in host.pending.drain(..) {
            match update {
                RetainedUpdate::Add { id, object } => {
                    handles.insert(id, gpu.add_instance(object, true)?);
                }
                RetainedUpdate::Remove(id) => {
                    gpu.remove_instance(handles.remove(&id).expect("validated object ID"))?;
                }
                RetainedUpdate::Replace { id, object } => {
                    let handle = handles.get_mut(&id).expect("validated object ID");
                    let old = gpu.instance(*handle)?.0;
                    if old.mesh() == object.mesh() && old.material() == object.material() {
                        gpu.set_instance_transform(*handle, *object.transform())?;
                    } else {
                        let replacement = gpu.add_instance(object, true)?;
                        gpu.remove_instance(*handle)?;
                        *handle = replacement;
                    }
                }
                RetainedUpdate::Settings(settings) => effects = Some(*settings),
                RetainedUpdate::Camera { .. } => {}
            }
        }
        Ok(effects)
    }
}

impl Renderer {
    pub fn take_surface_recreated(&mut self) -> bool {
        self.execution.take_recreated()
    }

    pub fn render_retained(&mut self, host: &mut RetainedHost) -> Result<bool, ViewerError> {
        let Some(view) = host.view()? else {
            return Ok(false);
        };
        let mut noop: crate::SceneUpdate = Box::new(|_| Ok(()));
        Ok(self
            .render_admitted(&view, &mut noop, None, Some(host))?
            .is_some())
    }

    pub(super) fn retained_settings(&mut self, effects: &SceneEffects) -> Result<(), ViewerError> {
        let mut settings = self.presentation.scheduler.configuration().clone();
        let upscale = settings.post.upscale;
        settings.post = worldrunner_render_scene::post_settings(effects)?;
        settings.post.upscale = upscale;
        if let Some((exposure, _)) = &mut settings.hdr {
            *exposure = f64::from(effects.grading().exposure_micro) / 1_000_000.0;
        }
        self.resident.set_retained_effects(effects)?;
        self.presentation
            .scheduler
            .reconfigure(&self.context, settings)
            .map_err(|error| ViewerError::Gpu(error.to_string()))?;
        self.presentation.refresh_targets();
        Ok(())
    }
}

impl Resident {
    pub(super) fn set_retained_effects(
        &mut self,
        effects: &SceneEffects,
    ) -> Result<(), ViewerError> {
        match self {
            Self::Shadowed(scene) => scene.set_effects(effects)?,
            Self::Plain(plain) => {
                plain.gpu.set_post_settings(effects)?;
                plain.effects = Some(effects.clone());
            }
        }
        Ok(())
    }
}
