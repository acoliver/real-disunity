use super::{Renderer, Resident};
use crate::{ObjectId, RetainedHost, RetainedUpdate, SceneObjectEdit, ViewerError};
use std::collections::BTreeMap;
use worldrunner_render_plan::SceneEffects;
use worldrunner_render_scene::draw::{InstanceHandle, SceneGpu};

#[derive(Default)]
pub(super) struct Objects {
    pub(super) handles: Option<BTreeMap<ObjectId, InstanceHandle>>,
}

impl Objects {
    pub(super) fn apply_updates(
        &mut self,
        gpu: &mut SceneGpu,
        updates: impl IntoIterator<Item = RetainedUpdate>,
    ) -> Result<Option<SceneEffects>, ViewerError> {
        let handles = self.handles.get_or_insert_with(|| {
            gpu.instance_handles()
                .enumerate()
                .map(|(index, handle)| (ObjectId(index as u64), handle))
                .collect()
        });
        let mut effects = None;
        for update in updates {
            match update {
                RetainedUpdate::Add { id, object } => {
                    apply_edit(handles, gpu, SceneObjectEdit::Add { id, object })?;
                }
                RetainedUpdate::Remove(id) => {
                    apply_edit(handles, gpu, SceneObjectEdit::Remove(id))?;
                }
                RetainedUpdate::Replace { id, object } => {
                    apply_edit(handles, gpu, SceneObjectEdit::Replace { id, object })?;
                }
                RetainedUpdate::Objects(edits) => {
                    for edit in edits {
                        apply_edit(handles, gpu, edit)?;
                    }
                }
                RetainedUpdate::Settings(settings) => effects = Some(*settings),
                RetainedUpdate::Camera { .. } => {}
                RetainedUpdate::Resources { .. }
                | RetainedUpdate::Terrain(_)
                | RetainedUpdate::Decal(_) => {
                    return Err(ViewerError::UnsupportedScene(
                        "scene replacement outside readiness",
                    ));
                }
            }
        }
        Ok(effects)
    }
}

/// Edits were validated against the host's object table before publication.
fn apply_edit(
    handles: &mut BTreeMap<ObjectId, InstanceHandle>,
    gpu: &mut SceneGpu,
    edit: SceneObjectEdit,
) -> Result<(), ViewerError> {
    match edit {
        SceneObjectEdit::Add { id, object } => {
            handles.insert(id, gpu.add_instance(object, true)?);
        }
        SceneObjectEdit::Remove(id) => {
            gpu.remove_instance(handles.remove(&id).expect("validated object ID"))?;
        }
        SceneObjectEdit::Replace { id, object } => {
            let handle = handles.get_mut(&id).expect("validated object ID");
            let old = gpu.instance(*handle)?.0;
            if old.mesh() == object.mesh() && old.material() == object.material() {
                gpu.set_instance_object(*handle, object)?;
            } else {
                let replacement = gpu.add_instance(object, true)?;
                gpu.remove_instance(*handle)?;
                *handle = replacement;
            }
        }
    }
    Ok(())
}

impl Renderer {
    pub(crate) fn presentation_budget_bytes(&self) -> u64 {
        self.presentation.scheduler.budget().0
    }

    pub(crate) fn take_statistics(&mut self) -> worldrunner_render_scene::draw::SceneStatistics {
        self.resident.scene_mut().take_statistics()
    }

    pub fn take_surface_recreated(&mut self) -> bool {
        self.execution.take_recreated()
    }

    pub fn collect_retained(&mut self, host: &mut RetainedHost) -> Result<bool, ViewerError> {
        self.collect_timing(host)?;
        self.streaming
            .collect(self.context.submission().completed_serial());
        self.streaming.captures.collect(host)?;
        Ok(self.streaming.captures.is_pending())
    }

    pub fn render_retained(&mut self, host: &mut RetainedHost) -> Result<bool, ViewerError> {
        self.poll()?;
        self.collect_retained(host)?;
        if self.render_recipe(host)?.is_some() {
            return Ok(false);
        }
        let Some(view) = host.view()? else {
            return Ok(false);
        };
        let mut noop: crate::SceneUpdate = Box::new(|_| Ok(()));
        Ok(self
            .render_admitted(&view, &mut noop, None, Some(host))?
            .is_some())
    }
}

pub(super) fn configure_effects(
    context: &super::GpuContext,
    resident: &mut Resident,
    presentation: &mut super::Presentation,
    effects: &SceneEffects,
) -> Result<(), ViewerError> {
    let mut settings = presentation.scheduler.configuration().clone();
    let upscale = settings.post.upscale;
    let native_rcas = settings.post.native_rcas;
    settings.post = worldrunner_render_scene::post_settings(effects)?;
    settings.post.upscale = upscale;
    settings.post.native_rcas = native_rcas;
    settings.ssao = effects.ssao().copied();
    settings.ssr = effects.ssr().copied();
    settings.water = effects.water_bodies().to_vec();
    if let Some((exposure, _)) = &mut settings.hdr {
        *exposure = f64::from(effects.grading().exposure_micro) / 1_000_000.0;
    }
    resident.set_retained_effects(effects)?;
    match resident {
        Resident::Shadowed(scene) => {
            scene.set_scene_depth_fitting(context, effects.depth_fitting())?;
        }
        Resident::Plain(_) if effects.depth_fitting().is_some() => {
            return Err(ViewerError::UnsupportedScene(
                "depth fitting requires directional shadows",
            ));
        }
        Resident::Plain(_) => {}
    }
    presentation.scheduler.reconfigure(context, settings)?;
    super::environment::install(context, resident, presentation, Some(effects), true)?;
    presentation.refresh_targets();
    Ok(())
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

pub(super) fn apply_decal(
    gpu: &mut SceneGpu,
    update: &crate::DecalUpdate,
) -> Result<(), ViewerError> {
    let mut placements: BTreeMap<_, _> = gpu.decal_placements().into_iter().collect();
    match update {
        crate::DecalUpdate::Add { id, decal } | crate::DecalUpdate::Replace { id, decal } => {
            placements.insert(*id, *decal);
        }
        crate::DecalUpdate::Remove(id) => {
            placements.remove(id);
        }
    }
    gpu.set_decal_placements(placements.into_iter().collect())?;
    Ok(())
}
