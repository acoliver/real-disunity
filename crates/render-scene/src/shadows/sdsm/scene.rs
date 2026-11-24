//! Stock geometry uses the shared metadata shaders before depth-driven fitting.
use super::{
    AdoptedFit, AdoptionPolicy, CaptureInput, DepthFitting, PendingReduction, SdsmError,
    SdsmSettings,
};
use crate::{
    compositor::ColorDepth,
    device::GpuContext,
    frame::{FrameUniform, metadata::FrameProduct},
    shadows::{ShadowError, ShadowedScene, frame::FitInput},
    view::SceneView,
};
use worldrunner_render_gpu::submission::capture::CaptureWrites;
use worldrunner_render_post::AttachmentView;

#[derive(Debug)]
pub(crate) struct SceneDepthFitting {
    settings: SdsmSettings,
    metadata: Option<crate::scene_metadata::SceneMetadata>,
    fitting: Option<DepthFitting>,
    pending: Vec<PendingReduction>,
    current: Option<AdoptedFit>,
    pub next_frame: u64,
    last_time: Option<std::time::Duration>,
}
impl ShadowedScene {
    /// Discards camera-dependent depth evidence and scene history after a camera cut.
    /// Frame identities remain monotonic; the next view gathers new fitting evidence.
    /// The host resets its own renderer's temporal history.
    pub fn reset_camera_history(&mut self) {
        self.gpu.reset_velocity_history();
        self.refresh.borrow_mut().invalidate();
        if let Some(state) = &mut self.depth_fitting {
            state.reset_history();
        }
    }

    pub fn set_scene_depth_fitting(
        &mut self,
        context: &GpuContext,
        settings: Option<&worldrunner_render_plan::SceneDepthFitting>,
    ) -> Result<(), ShadowError> {
        if let Some(settings) = settings {
            settings
                .validate()
                .map_err(|_| ShadowError::InvalidSettings)?;
        }
        let settings = settings.map(SdsmSettings::from);
        if settings != self.depth_fitting_settings() {
            self.set_depth_fitting(context, settings)?;
        }
        Ok(())
    }

    /// Opt in to stock metadata-driven shadows. None keeps the original pass plan.
    pub fn set_depth_fitting(
        &mut self,
        _context: &GpuContext,
        settings: Option<SdsmSettings>,
    ) -> Result<(), ShadowError> {
        if let Some(settings) = settings {
            super::AdoptionState::new(settings)?;
            self.depth_fitting = Some(SceneDepthFitting {
                settings,
                metadata: None,
                fitting: None,
                pending: Vec::new(),
                current: None,
                next_frame: 0,
                last_time: None,
            });
        } else {
            self.depth_fitting = None;
        }
        Ok(())
    }

    pub fn last_depth_fit(&self) -> Option<AdoptedFit> {
        self.depth_fitting.as_ref().and_then(|state| state.current)
    }

    pub fn depth_fitting_settings(&self) -> Option<SdsmSettings> {
        self.depth_fitting.as_ref().map(|state| state.settings)
    }

    /// Fits with a host's retained stock frame profile, including HDR and screen effects,
    /// replaying requested opaque terms in the fitted frame's drawing transaction.
    /// Same-frame capture submits only metadata and reduction before refitting.
    /// Interactive mode polls completed evidence without waiting for the GPU.
    pub fn render_depth_fitted_host(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        frame: (Option<u64>, bool, &[crate::compositor::terms::TermCapture]),
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<Option<AdoptedFit>, ShadowError> {
        let mut state = self
            .depth_fitting
            .take()
            .ok_or(ShadowError::UnsupportedFeature("depth fitting is disabled"))?;
        let policy = state.settings.policy;
        if frame.1 {
            state.settings.policy = AdoptionPolicy::SameFrameCapture;
        }
        let work = FrameWork {
            source_frame: frame.0.unwrap_or(state.next_frame),
            terms: frame.2,
            scheduler: &mut *scheduler,
        };
        let result = self.render_sdsm(context, view, attachments, work, &mut state);
        scheduler.clear_capture_resources();
        self.gpu.clear_capture_pages();
        state.settings.policy = policy;
        self.depth_fitting = Some(state);
        result
    }

    fn render_sdsm(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        work: FrameWork<'_>,
        state: &mut SceneDepthFitting,
    ) -> Result<Option<AdoptedFit>, ShadowError> {
        let FrameWork {
            source_frame,
            terms,
            scheduler,
        } = work;
        self.prepare_sdsm_producers(context, attachments.1, source_frame, state, scheduler)?;
        let metadata = state.bind(context, scheduler, attachments.0.size())?;
        let frame = scheduler.prepare_uniform(FrameUniform::new(view, Some(&self.effects))?)?;
        let input = RenderInput {
            context,
            view,
            attachments,
            source_frame,
            frame,
            metadata,
            terms,
        };
        let mut fitting = state.fitting.take().expect("fitting installed");
        fitting.set_timing(scheduler.timing_state());
        let result = match state.settings.policy {
            AdoptionPolicy::SameFrameCapture => {
                let result = self.render_capture(&input, state, &fitting, scheduler);
                fitting.invalidate_pending();
                state.pending.clear();
                result
            }
            AdoptionPolicy::Interactive { .. } | AdoptionPolicy::QuantizedPerBound { .. } => {
                self.render_interactive(&input, state, &mut fitting, scheduler)
            }
        };
        if result.is_err() {
            fitting.invalidate();
            state.pending.clear();
            state.current = None;
        }
        state.fitting = Some(fitting);
        if let Ok(fit) = &result {
            state.current = *fit;
            state.next_frame = source_frame
                .checked_add(1)
                .ok_or(SdsmError::FrameIdentity)?;
        }
        result
    }

    fn prepare_sdsm_producers(
        &mut self,
        context: &GpuContext,
        depth: &AttachmentView,
        source_frame: u64,
        state: &mut SceneDepthFitting,
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<(), ShadowError> {
        if source_frame < state.next_frame {
            return Err(SdsmError::FrameIdentity.into());
        }
        let requested = u32::try_from(self.effects.shadows().extents_millimeters().len())
            .expect("four cascades");
        if requested != self.map.depth.layers() {
            return Err(SdsmError::CascadeLayers {
                allocated: self.map.depth.layers(),
                requested,
            }
            .into());
        }
        state.select_time(scheduler.selected_time);
        scheduler.prepare_receiver_gradients(
            context.device(),
            &self.gpu,
            self.settings.normal_bias_meters,
        )?;
        scheduler.prepare_scene_inputs(context.device(), &self.gpu);
        let depth = scheduler.forward_depth(&self.gpu, depth);
        let selected = scheduler.receiver_pipelines(&self.pipelines, depth);
        let velocity = selected.writes_velocity();
        self.gpu.track_previous_transforms(velocity);
        Ok(())
    }

    fn capture_contract(
        &mut self,
        input: &RenderInput<'_>,
        scheduler: &mut crate::compositor::FrameRenderer,
        key: worldrunner_render_gpu::submission::capture::WriteKey,
    ) -> Result<CaptureWrites, ShadowError> {
        scheduler.prepare_scene_inputs(input.context.device(), &self.gpu);
        let scene_keys = self.gpu.capture_write_keys(input.context.device())?;
        let mut suffix = scene_keys.clone();
        suffix.push(self.gpu.instance_history_key());
        suffix.push(crate::compositor::publication::key(&self.refresh));
        suffix.extend(scheduler.capture_write_keys());
        suffix.extend(scheduler.capture_effect_keys(input.context.device())?);
        suffix.extend(scheduler.capture_contour_keys(
            input.context.device(),
            &self.gpu,
            input.attachments.1,
        )?);
        let celestial = self
            .gpu
            .capture_celestial_textures(input.context.device(), input.attachments.0.size());
        suffix.extend(scheduler.capture_texture_keys(&celestial.iter().collect::<Vec<_>>())?);
        suffix.extend(scheduler.capture_texture_keys(&[
            input.attachments.0.texture(),
            input.attachments.1.texture(),
            self.map.depth.texture(),
        ])?);
        for term in input.terms {
            suffix.extend(scheduler.capture_texture_keys(&term.textures())?);
        }
        let prefix = Self::capture_prefix_keys(scheduler, scene_keys)?;
        let metadata = Self::capture_metadata_keys(input, scheduler)?;
        let scene_depth = scheduler.capture_texture_keys(&[input.attachments.1.texture()])?[0];
        Ok(capture_writes(prefix, suffix, metadata, scene_depth, key).map_err(SdsmError::from)?)
    }

    fn capture_prefix_keys(
        scheduler: &mut crate::compositor::FrameRenderer,
        keys: Vec<worldrunner_render_gpu::submission::capture::WriteKey>,
    ) -> Result<Vec<worldrunner_render_gpu::submission::capture::WriteKey>, ShadowError> {
        let mut prefix = keys;
        prefix.extend(scheduler.capture_prefix_game_keys()?);
        Ok(prefix)
    }

    fn capture_metadata_keys(
        input: &RenderInput<'_>,
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<Vec<worldrunner_render_gpu::submission::capture::WriteKey>, ShadowError> {
        // With reflections installed the prefix prepass also writes the reflection material.
        let material = scheduler
            .ssr
            .as_ref()
            .map(|ssr| ssr.material_target().clone());
        let mut written = vec![
            &input.metadata.normals,
            &input.metadata.depth,
            input.metadata.passes.depth_levels()[0],
        ];
        written.extend(material.as_ref());
        if let Some(receiver) = &input.metadata.receiver {
            written.extend(receiver.textures.iter());
        }
        Ok(scheduler.capture_texture_keys(&written)?)
    }

    fn encode_fitted_prefix(
        &mut self,
        input: &RenderInput<'_>,
        scheduler: &mut crate::compositor::FrameRenderer,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), ShadowError> {
        scheduler
            .begin_timing(input.source_frame)
            .map_err(worldrunner_render_post::FrameError::from)?;
        scheduler.encode_depth_prefix(
            input.context.device(),
            encoder,
            crate::compositor::PrefixScene {
                gpu: &mut self.gpu,
                pipelines: &self.pipelines,
            },
            &input.frame,
            &input.metadata,
        )?;
        Ok(())
    }

    fn render_capture(
        &mut self,
        input: &RenderInput<'_>,
        state: &SceneDepthFitting,
        fitting: &DepthFitting,
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<Option<AdoptedFit>, ShadowError> {
        self.request_shadow_capture();
        let context = input.context;
        drain_capture(context)?;
        let key = fitting.reducer().write_key();
        let contract = self.capture_contract(input, scheduler, key)?;
        let prefix = fitting
            .reducer()
            .begin_capture(
                context,
                (contract, key),
                CaptureInput {
                    frame: *input.view,
                    source_frame: input.source_frame,
                    clip_meters: input.view.resolved_view().clip_distances_meters(),
                    range: state.settings.range,
                },
                fitting.ticket(input.source_frame),
                |encoder, _, _| {
                    self.encode_fitted_prefix(input, scheduler, encoder)?;
                    Ok::<_, ShadowError>(())
                },
            )
            .map_err(capture_error)?;
        let continuation = prefix.submit().map_err(capture_error)?;
        let mut adopted = None;
        let suffix = continuation
            .resume(context.device(), |encoder, _, frozen, fit| {
                let fitted = FitInput {
                    scheduler: &mut *scheduler,
                    fitting: Some((fit.depth, state.settings.range)),
                };
                let attachments = ColorDepth {
                    color: input.attachments.0,
                    depth: input.attachments.1,
                };
                self.encode_fitted(context, frozen, attachments, encoder, fitted)?;
                self.encode_term_captures(context.device(), encoder, input.terms, scheduler)?;
                adopted = Some(fit);
                Ok::<_, ShadowError>(())
            })
            .map_err(capture_error)?;
        suffix
            .submit()
            .map_err(capture_error)?
            .finish(context.device(), |_, _| Ok::<_, ShadowError>(()))
            .map_err(capture_error)?;
        Ok(adopted)
    }

    fn render_interactive(
        &mut self,
        input: &RenderInput<'_>,
        state: &mut SceneDepthFitting,
        fitting: &mut DepthFitting,
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<Option<AdoptedFit>, ShadowError> {
        let context = input.context;
        let mut index = 0;
        while index < state.pending.len() {
            if let Some(depth) = state.pending[index].poll(context.device())? {
                let pending = state.pending.remove(index);
                fitting.complete(&pending.ticket, Ok(depth))?;
            } else {
                index += 1;
            }
        }
        let clip = input.view.resolved_view().clip_distances_meters();
        let adopted = if let Some(time) = state.last_time {
            fitting.adopt_at(input.source_frame, clip, time)?
        } else {
            fitting.adopt(input.source_frame, clip)?
        };
        let current = adopted.or(state.current);
        let mut pending = None;
        let prepared = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                self.encode_fitted_prefix(input, scheduler, encoder)?;
                pending = Some(fitting.encode(
                    context.device(),
                    encoder,
                    input.source_frame,
                    fitting.reducer().write_key(),
                )?);
                let attachments = ColorDepth {
                    color: input.attachments.0,
                    depth: input.attachments.1,
                };
                let fitting = FitInput {
                    scheduler: &mut *scheduler,
                    fitting: current.map(|fit| (fit.depth, state.settings.range)),
                };
                self.encode_fitted(context, input.view, attachments, encoder, fitting)?;
                self.encode_term_captures(context.device(), encoder, input.terms, scheduler)?;
                Ok::<_, ShadowError>(())
            })
            .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
        prepared.submit()?;
        let mut pending = pending.expect("encoded reduction");
        pending.begin_mapping()?;
        state.pending.push(pending);
        Ok(current)
    }
}
/// The frame a depth-fitted render draws: its identity, the opaque terms replayed in its
/// drawing transaction and the host's renderer it schedules through.
struct FrameWork<'a> {
    source_frame: u64,
    terms: &'a [crate::compositor::terms::TermCapture],
    scheduler: &'a mut crate::compositor::FrameRenderer,
}
struct RenderInput<'a> {
    context: &'a GpuContext,
    view: &'a SceneView,
    attachments: (&'a AttachmentView, &'a AttachmentView),
    source_frame: u64,
    frame: FrameUniform,
    metadata: crate::scene_metadata::SceneMetadata,
    terms: &'a [crate::compositor::terms::TermCapture],
}
impl SceneDepthFitting {
    fn reset_history(&mut self) {
        if let Some(fitting) = &mut self.fitting {
            fitting.invalidate();
        }
        self.pending.clear();
        self.current = None;
    }

    fn select_time(&mut self, time: Option<std::time::Duration>) {
        if time
            .zip(self.last_time)
            .is_some_and(|(now, last)| now < last)
        {
            self.reset_history();
        }
        self.last_time = time;
    }
    fn bind(
        &mut self,
        context: &GpuContext,
        scheduler: &crate::compositor::FrameRenderer,
        size: [u32; 2],
    ) -> Result<crate::scene_metadata::SceneMetadata, ShadowError> {
        // An HDR frame configured with depth fitting owns this prepass and pairs its depth with
        // Opaque. Only a frame without screen-space metadata leaves fitting to draw its own.
        if scheduler.ssao.is_none()
            && self
                .metadata
                .as_ref()
                .is_none_or(|source| source.size != size)
        {
            self.metadata = Some(crate::scene_metadata::SceneMetadata::new(
                context.device(),
                size,
                crate::ssr::neutral_ao(),
            )?);
        }
        let metadata = scheduler
            .ssao
            .as_ref()
            .or(self.metadata.as_ref())
            .expect("metadata installed")
            .clone();
        let depth = FrameProduct::linear_depth(0).attachment(metadata.passes.depth_levels()[0])?;
        let normals = FrameProduct::normals().attachment(&metadata.normals)?;
        if let Some(fitting) = &mut self.fitting {
            if fitting.rebind(context.device(), (&depth, &normals))? {
                self.pending.clear();
                self.current = None;
            }
        } else {
            self.fitting = Some(DepthFitting::new(
                context.device(),
                self.settings,
                (&depth, &normals),
            )?);
        }
        Ok(metadata)
    }
}
fn capture_error<E>(
    error: worldrunner_render_gpu::submission::capture::CaptureError<E>,
) -> ShadowError
where
    ShadowError: From<E>,
{
    use worldrunner_render_gpu::submission::capture::CaptureError;
    match error {
        CaptureError::Submission(error) => error.into(),
        CaptureError::Write(error) => SdsmError::from(error).into(),
        CaptureError::Poll(error) => SdsmError::from(error).into(),
        CaptureError::Preparation(error) | CaptureError::Readback(error) => error.into(),
    }
}

fn drain_capture(context: &GpuContext) -> Result<(), ShadowError> {
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .map_err(SdsmError::from)?;
    context
        .submission()
        .drain_captures(context.device())
        .map_err(capture_error)
}

fn capture_writes(
    mut prefix: Vec<worldrunner_render_gpu::submission::capture::WriteKey>,
    mut suffix: Vec<worldrunner_render_gpu::submission::capture::WriteKey>,
    metadata: Vec<worldrunner_render_gpu::submission::capture::WriteKey>,
    scene_depth: worldrunner_render_gpu::submission::capture::WriteKey,
    reduction: worldrunner_render_gpu::submission::capture::WriteKey,
) -> Result<CaptureWrites, worldrunner_render_gpu::submission::capture::WriteError> {
    // Water writes depth after the metadata prefix.
    suffix.retain(|key| *key == scene_depth || !metadata.contains(key));
    prefix.extend(metadata);
    prefix.push(reduction);
    let owned = prefix
        .iter()
        .copied()
        .filter(|key| suffix.contains(key))
        .collect::<Vec<_>>();
    CaptureWrites::new(prefix, suffix, owned)
}

#[cfg(test)]
#[path = "capture_contract_tests.rs"]
mod contract_tests;
