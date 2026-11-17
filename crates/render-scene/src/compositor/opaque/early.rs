//! Admission of the early material producer before allocating or replacing frame state.
use super::{OpaqueAdmissionError, admit_path};
use crate::compositor::{ReceiverGradientReservation, RendererConfiguration};
use crate::gbuffer_writes::EarlyOutputs;
use worldrunner_render_plan::{SceneEffects, SceneOpaquePath};
use worldrunner_render_post::{FrameConfig, FrameError, HdrPostSettings, PostError};

impl From<OpaqueAdmissionError> for PostError {
    fn from(error: OpaqueAdmissionError) -> Self {
        Self::Frame(frame_error(&error))
    }
}

fn frame_error(error: &OpaqueAdmissionError) -> FrameError {
    FrameError::FeaturePreparation {
        feature: "opaque G-buffer",
        reason: error.to_string(),
    }
}

/// Rejects only Render's unsupported upper-bound reservation. Hosts derive this reservation
/// from resolved shadow settings: None without shadows, Front at zero bias, Opposing otherwise.
/// The decision never depends on the initial scene's current receiving materials.
pub fn validate_receiver_reservation(
    path: SceneOpaquePath,
    reservation: ReceiverGradientReservation,
) -> Result<(), OpaqueAdmissionError> {
    if path == SceneOpaquePath::GBUFFER_RENDER
        && reservation == ReceiverGradientReservation::Opposing
    {
        Err(OpaqueAdmissionError::UnsupportedOpposingReceiver)
    } else {
        Ok(())
    }
}

pub(crate) fn admit_early_producer(
    path: SceneOpaquePath,
    samples: u32,
    reservation: ReceiverGradientReservation,
    outputs: EarlyOutputs,
    limits: &wgpu::Limits,
) -> Result<(), PostError> {
    admit_path(path, samples)?;
    validate_receiver_reservation(path, reservation)?;
    if path != SceneOpaquePath::GBUFFER_RENDER {
        return Ok(());
    }
    validate_outputs(outputs, limits).map_err(PostError::from)
}

fn validate_outputs(
    outputs: EarlyOutputs,
    limits: &wgpu::Limits,
) -> Result<(), OpaqueAdmissionError> {
    let formats = outputs.formats();
    let count = u32::try_from(formats.len()).expect("bounded early producer outputs");
    let bytes = color_attachment_byte_cost(&formats);
    if count > limits.max_color_attachments {
        return Err(OpaqueAdmissionError::ColorAttachments {
            required: count,
            available: limits.max_color_attachments,
        });
    }
    if bytes > limits.max_color_attachment_bytes_per_sample {
        return Err(OpaqueAdmissionError::ColorAttachmentBytes {
            required: bytes,
            available: limits.max_color_attachment_bytes_per_sample,
        });
    }
    Ok(())
}

pub(super) fn color_attachment_byte_cost(formats: &[wgpu::TextureFormat]) -> u32 {
    formats.iter().fold(0u32, |total, format| {
        total.next_multiple_of(format.target_component_alignment().unwrap())
            + format.target_pixel_byte_cost().unwrap()
    })
}

impl RendererConfiguration {
    /// Checks path, receiver representation and ordered producer outputs without allocation.
    /// Pass enabled device limits; adapter maxima do not establish an enabled capability.
    pub fn validate_opaque(&self, limits: &wgpu::Limits) -> Result<(), PostError> {
        admit_early_producer(
            self.opaque_path,
            self.frame.samples,
            self.receiver_gradients,
            EarlyOutputs {
                velocity: self.post.resolve.active_taa().is_some(),
                reflections: self.ssr.is_some_and(|settings| settings.enabled),
            },
            limits,
        )
    }
}

/// Low-level HDR constructors have no shadow settings. Check their physical outputs now;
/// actual shadow normal bias is checked before receiver preparation at frame encoding.
pub(crate) fn validate_hdr_admission(
    device: &wgpu::Device,
    config: FrameConfig,
    post: &HdrPostSettings,
    effects: Option<&SceneEffects>,
) -> Result<(), PostError> {
    admit_early_producer(
        effects.map_or(SceneOpaquePath::Forward, SceneEffects::opaque_path),
        config.samples,
        ReceiverGradientReservation::None,
        EarlyOutputs {
            velocity: post.resolve.active_taa().is_some(),
            reflections: effects
                .and_then(SceneEffects::ssr)
                .is_some_and(|ssr| ssr.enabled),
        },
        &device.limits(),
    )
}

impl crate::compositor::FrameRenderer {
    pub(crate) fn validate_opaque_normal_bias(&self, bias: f32) -> Result<(), FrameError> {
        let path = if self
            .gbuffer
            .as_ref()
            .is_some_and(crate::gbuffer::Gbuffer::renders)
        {
            SceneOpaquePath::GBUFFER_RENDER
        } else {
            SceneOpaquePath::Forward
        };
        let reservation = if bias == 0.0 {
            ReceiverGradientReservation::Front
        } else {
            ReceiverGradientReservation::Opposing
        };
        validate_receiver_reservation(path, reservation).map_err(|error| frame_error(&error))
    }
}
