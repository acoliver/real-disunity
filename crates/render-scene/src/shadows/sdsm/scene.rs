//! Stock geometry uses the shared metadata shaders before depth-driven fitting.
use super::{
    AdoptedFit, AdoptionPolicy, CaptureInput, DepthFitting, PendingReduction, SdsmError,
    SdsmSettings,
};
use crate::{
    device::GpuContext,
    frame::{FrameUniform, metadata::FrameProduct},
    shadows::{ShadowError, ShadowedScene},
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
}
impl ShadowedScene {
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
            });
        } else {
            self.depth_fitting = None;
        }
        Ok(())
    }

    /// Same-frame capture submits only metadata and reduction before refitting.
    /// Interactive mode polls completed evidence without waiting for the GPU.
    pub fn render_depth_fitted(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        source_frame: u64,
    ) -> Result<Option<AdoptedFit>, ShadowError> {
        let mut state = self
            .depth_fitting
            .take()
            .ok_or(ShadowError::UnsupportedFeature("depth fitting is disabled"))?;
        let result = self.render_sdsm(context, view, attachments, source_frame, &mut state);
        self.scheduler.clear_capture_resources();
        self.gpu.clear_capture_pages();
        self.depth_fitting = Some(state);
        result
    }

    /// Uses a host's retained stock frame profile, including HDR and screen effects.
    pub fn render_depth_fitted_with(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        source_frame: u64,
        scheduler: &mut crate::compositor::FrameRenderer,
    ) -> Result<Option<AdoptedFit>, ShadowError> {
        std::mem::swap(&mut self.scheduler, scheduler);
        let result = self.render_depth_fitted(context, view, attachments, source_frame);
        std::mem::swap(&mut self.scheduler, scheduler);
        result
    }

    fn render_sdsm(
        &mut self,
        context: &GpuContext,
        view: &SceneView,
        attachments: (&AttachmentView, &AttachmentView),
        source_frame: u64,
        state: &mut SceneDepthFitting,
    ) -> Result<Option<AdoptedFit>, ShadowError> {
        let requested = u32::try_from(self.effects.shadows().extents_millimeters().len())
            .expect("four cascades");
        if requested != self.map.depth.layers() {
            return Err(SdsmError::CascadeLayers {
                allocated: self.map.depth.layers(),
                requested,
            }
            .into());
        }
        let metadata = state.bind(context, &self.scheduler, attachments.0.size())?;
        let frame = self
            .scheduler
            .prepare_uniform(FrameUniform::new(view, Some(&self.effects))?)?;
        let input = RenderInput {
            context,
            view,
            attachments,
            source_frame,
            frame,
            metadata,
        };
        let mut fitting = state.fitting.take().expect("fitting installed");
        let result = match state.settings.policy {
            AdoptionPolicy::SameFrameCapture => self.render_capture(&input, state, &fitting),
            AdoptionPolicy::Interactive { .. } => {
                self.render_interactive(&input, state, &mut fitting)
            }
        };
        state.fitting = Some(fitting);
        if let Ok(fit) = &result {
            state.current = *fit;
            state.next_frame = source_frame
                .checked_add(1)
                .ok_or(SdsmError::FrameIdentity)?;
        }
        result
    }

    fn capture_contract(
        &mut self,
        input: &RenderInput<'_>,
        key: worldrunner_render_gpu::submission::capture::WriteKey,
    ) -> Result<CaptureWrites, ShadowError> {
        let scene_keys = self.gpu.capture_write_keys(input.context.device())?;
        let mut suffix = scene_keys.clone();
        suffix.extend(self.scheduler.capture_write_keys());
        suffix.extend(self.scheduler.capture_effect_keys(input.context.device())?);
        suffix.extend(self.scheduler.capture_texture_keys(&[
            input.attachments.0.texture(),
            input.attachments.1.texture(),
            self.map.depth.texture(),
        ])?);
        let mut prefix = scene_keys;
        let metadata = self.scheduler.capture_texture_keys(&[
            &input.metadata.normals,
            &input.metadata.depth,
            input.metadata.passes.depth_levels()[0],
        ])?;
        suffix.retain(|key| !metadata.contains(key));
        prefix.extend(metadata);
        prefix.push(key);
        Ok(CaptureWrites::new(prefix, suffix.clone(), suffix).map_err(SdsmError::from)?)
    }

    fn render_capture(
        &mut self,
        input: &RenderInput<'_>,
        state: &SceneDepthFitting,
        fitting: &DepthFitting,
    ) -> Result<Option<AdoptedFit>, ShadowError> {
        let context = input.context;
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
            .map_err(capture_error)?;
        let key = fitting.reducer().write_key();
        let contract = self.capture_contract(input, key)?;
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
                    self.scheduler.encode_depth_prefix(
                        context.device(),
                        encoder,
                        (&mut self.gpu, &self.pipelines),
                        input.frame,
                        &input.metadata,
                    )?;
                    Ok::<_, ShadowError>(())
                },
            )
            .map_err(capture_error)?;
        let continuation = prefix.submit().map_err(capture_error)?;
        let mut adopted = None;
        let suffix = continuation
            .resume(context.device(), |encoder, _, frozen, fit| {
                self.encode_depth_fitted(
                    context,
                    frozen,
                    input.attachments,
                    encoder,
                    (fit, state.settings.range),
                )?;
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
        let current = fitting
            .adopt(
                input.source_frame,
                input.view.resolved_view().clip_distances_meters(),
            )?
            .or(state.current);
        let mut pending = None;
        let prepared = context
            .submission()
            .prepare(context.device(), |encoder, _| {
                self.scheduler.encode_depth_prefix(
                    context.device(),
                    encoder,
                    (&mut self.gpu, &self.pipelines),
                    input.frame,
                    &input.metadata,
                )?;
                pending = Some(fitting.encode(
                    context.device(),
                    encoder,
                    input.source_frame,
                    fitting.reducer().write_key(),
                )?);
                self.encode_fitted(
                    context,
                    input.view,
                    input.attachments,
                    encoder,
                    (None, current.map(|fit| (fit.depth, state.settings.range))),
                )
            })
            .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
        prepared.submit()?;
        let mut pending = pending.expect("encoded reduction");
        pending.begin_mapping()?;
        state.pending.push(pending);
        Ok(current)
    }
}
struct RenderInput<'a> {
    context: &'a GpuContext,
    view: &'a SceneView,
    attachments: (&'a AttachmentView, &'a AttachmentView),
    source_frame: u64,
    frame: FrameUniform,
    metadata: crate::scene_metadata::SceneMetadata,
}
impl SceneDepthFitting {
    fn bind(
        &mut self,
        context: &GpuContext,
        scheduler: &crate::compositor::FrameRenderer,
        size: [u32; 2],
    ) -> Result<crate::scene_metadata::SceneMetadata, ShadowError> {
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
