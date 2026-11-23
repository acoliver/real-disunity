use super::{Renderer, Resident};
use crate::{ObjectId, RetainedHost, RetainedUpdate, SceneObjectEdit, ViewerError};
use std::collections::BTreeMap;
use worldrunner_render_gpu::flight::CompletionTicket;
use worldrunner_render_plan::SceneEffects;
use worldrunner_render_scene::{
    SceneView,
    draw::{InstanceHandle, SceneGpu},
};

pub(super) fn finish_admitted_frame(
    renderer: &mut Renderer,
    host: Option<&mut RetainedHost>,
    view: &SceneView,
    retained_capture: Option<(u64, super::capture::Products)>,
    ticket: CompletionTicket,
) -> Result<(), ViewerError> {
    if let Some(host) = host {
        renderer.submitted_camera_history(host);
        let temporal_history_reset = renderer.temporal_history_reset();
        host.present_pick(view.resolved_view());
        let statistics = renderer.scene_statistics();
        let frame = renderer.streaming.captures.next_frame();
        renderer.streaming.captures.submitted(
            host,
            retained_capture,
            ticket,
            &crate::retained::CaptureCompletion {
                frame,
                time: renderer.frame_time,
                scene: statistics,
                temporal_history_reset,
            },
        )?;
    } else {
        renderer.streaming.captures.advance_frame();
    }
    renderer.check_errors()?;
    Ok(())
}

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
                RetainedUpdate::Camera { .. }
                | RetainedUpdate::CameraWithContinuity { .. }
                | RetainedUpdate::CameraWithShadowPolicy { .. } => {}
                RetainedUpdate::Ui(_) | RetainedUpdate::World(_) => {
                    unreachable!("UI and world updates publish through readiness")
                }
                RetainedUpdate::Resources { .. }
                | RetainedUpdate::AppendResources(_)
                | RetainedUpdate::ReplaceTexture { .. }
                | RetainedUpdate::ReplaceMesh { .. }
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
    pub(crate) fn temporal_history_reset(&self) -> crate::TemporalHistoryReset {
        self.streaming.temporal_history_reset
    }

    pub(super) fn prepare_camera_history(&mut self, host: &RetainedHost) {
        host.readiness.camera_history.prepare(|| {
            self.presentation.scheduler.reset_temporal_history();
            match &mut self.resident {
                Resident::Shadowed(scene) => scene.reset_camera_history(),
                Resident::Plain(scene) => scene.gpu.reset_velocity_history(),
            }
        });
    }

    pub(super) fn submitted_camera_history(&mut self, host: &mut RetainedHost) {
        self.streaming.temporal_history_reset = host.readiness.camera_history.reset_reason();
        host.readiness.camera_history.submitted();
    }

    pub(crate) fn submitted_frame(&self) -> u64 {
        self.streaming
            .captures
            .next_frame()
            .checked_sub(1)
            .expect("submitted frame")
    }

    pub(crate) fn capture_pool_bytes(&self) -> u64 {
        self.presentation.capture_pool_bytes()
    }

    pub(crate) fn presentation_budget_bytes(&self) -> u64 {
        self.presentation.scheduler.budget().0
    }

    /// The texture tier chosen for the resident scene, when the host set a policy.
    pub(crate) fn texture_tier(&self) -> Option<worldrunner_render_scene::texture::TierChoice> {
        self.streaming.tiers.chosen
    }

    #[cfg(test)]
    pub(crate) fn stock_plan_statistics(
        &self,
    ) -> worldrunner_render_scene::compositor::StockPlanStatistics {
        self.presentation.scheduler.stock_plan_statistics()
    }

    /// Resident GPU bytes the device's census counts, by resource class.
    pub(crate) fn resident_bytes(&self) -> worldrunner_render_gpu::census::CensusSnapshot {
        self.context.submission().census().snapshot()
    }

    /// Counters of the device's reusable upload staging.
    pub(crate) fn upload_staging(&self) -> worldrunner_render_gpu::submission::StagingStatistics {
        self.context.submission().upload_staging()
    }

    /// The frame's statistics, consuming its uploads and those of residents it replaced.
    pub(crate) fn take_statistics(&mut self) -> worldrunner_render_scene::draw::SceneStatistics {
        let carried = std::mem::take(&mut self.streaming.carried);
        carried.applied(self.resident.scene_mut().take_statistics())
    }

    /// The frame's statistics without consuming anything.
    pub(crate) fn scene_statistics(&mut self) -> worldrunner_render_scene::draw::SceneStatistics {
        self.streaming
            .carried
            .applied(self.resident.scene_mut().statistics())
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

    pub(super) fn reject_offscreen_depth_fitting(
        &self,
        host: &RetainedHost,
    ) -> Result<(), ViewerError> {
        if host.offscreen.is_some() && self.resident.depth_fitting_enabled() {
            return Err(ViewerError::UnsupportedScene(
                "player offscreen work with same-frame depth fitting",
            ));
        }
        Ok(())
    }

    pub fn render_retained(&mut self, host: &mut RetainedHost) -> Result<bool, ViewerError> {
        self.poll()?;
        self.collect_retained(host)?;
        if self.render_recipe(host)?.is_some() {
            return Ok(false);
        }
        if !host.has_view_extent() {
            return Ok(false);
        }
        Ok(self.render_admitted(host)?.is_some())
    }
}

pub(super) fn configure_effects(
    context: &super::GpuContext,
    resident: &mut Resident,
    presentation: &mut super::Presentation,
    effects: &SceneEffects,
) -> Result<(), ViewerError> {
    let settings = preflight_effects(presentation, effects)?;
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

/// Resolves the same candidate used for replacement, without touching resident or scheduler.
pub(super) fn preflight_effects(
    presentation: &super::Presentation,
    effects: &SceneEffects,
) -> Result<worldrunner_render_scene::compositor::RendererConfiguration, ViewerError> {
    let mut settings = presentation.scheduler.configuration().clone();
    let upscale = settings.post.upscale;
    let native_rcas = settings.post.native_rcas;
    settings.post = worldrunner_render_scene::post_settings(effects)?;
    settings.post.upscale = upscale;
    settings.post.native_rcas = native_rcas;
    settings.ssao = effects.ssao().copied();
    settings.ssr = effects.ssr().copied();
    settings.depth_fitting = effects.depth_fitting().copied();
    settings.water = effects.water_bodies().to_vec();
    settings.hdr.0 = f64::from(effects.grading().exposure_micro) / 1_000_000.0;
    Ok(settings)
}

/// Check before consuming preparation results or initializing object-handle bookkeeping.
pub(super) fn preflight_preparation(
    presentation: &super::Presentation,
    front: &crate::retained::readiness::Preparation,
) -> Result<(), ViewerError> {
    let effects = match &front.update {
        crate::RetainedUpdate::Settings(effects) => Some(effects.as_ref()),
        _ => front.pick_scene.as_ref().and_then(|scene| scene.effects()),
    };
    if let Some(effects) = effects {
        preflight_effects(presentation, effects)?;
    }
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
                plain.gpu.set_ssr_settings(effects.ssr().copied())?;
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

pub(super) fn apply_ui(
    context: &super::GpuContext,
    resident: &mut Resident,
    presentation: &mut super::Presentation,
    overlay: &std::sync::Arc<worldrunner_render_scene::simulation::UiOverlay>,
    targets: &[worldrunner_render_scene::simulation::UiTarget],
) -> Result<(), ViewerError> {
    let mut settings = presentation.scheduler.configuration().clone();
    if settings.ui_depth != overlay.needs_depth() {
        settings.ui_depth = overlay.needs_depth();
        presentation.scheduler.reconfigure(context, settings)?;
        presentation.refresh_targets();
    }
    resident
        .scene_mut()
        .set_ui_overlay(context.device(), context.submission(), overlay, targets)
        .map_err(|error| ViewerError::Gpu(error.to_string()))
}

pub(super) fn apply_world(
    context: &super::GpuContext,
    resident: &mut Resident,
    frame: &worldrunner_render_scene::simulation::WorldSceneContribution,
) -> Result<(), ViewerError> {
    let Resident::Plain(plain) = resident else {
        return Err(ViewerError::UnsupportedScene("dynamic shadow host"));
    };
    plain.gpu.set_dynamic_contribution(
        context.device(),
        context.submission(),
        &plain.pipelines,
        &frame.textures,
        &frame.quads,
    )?;
    let [r, g, b, a] = frame.clear;
    plain.clear = Some(wgpu::Color { r, g, b, a });
    Ok(())
}

impl Resident {
    /// The colour a world frame clears to, when the resident has shown one.
    pub(super) fn clear_color(&self) -> Option<wgpu::Color> {
        match self {
            Self::Plain(plain) => plain.clear,
            Self::Shadowed(_) => None,
        }
    }
}
