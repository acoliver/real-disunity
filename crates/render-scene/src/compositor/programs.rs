//! Player program draws in the stock opaque pass.
//!
//! The scene's feature records admitted draws after its own opaque geometry, in the same pass,
//! so they share its attachments, its depth buffer and the frame's one submission. This first
//! slice covers a pass with one single-sample color attachment and `Depth32Float`. Passes that
//! add attachments or samples are refused by name until their behavior has its own tests.

use super::FrameRenderer;
use crate::draw::SceneGpu;
use crate::renderer::ScenePipelines;
use worldrunner_render_post::FrameError;
use worldrunner_render_program::error::{ProgramError, UnsupportedTarget};
use worldrunner_render_program::runtime::{AdmittedPrograms, TargetSignature};

const FEATURE: &str = "player programs";

/// The depth format the scene declares for its opaque pass.
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Takes the draws admitted to `gpu`, checks them against the frame about to be declared and
/// has the frame retain their arena. `frame` is the scene pipelines and whether the frame writes
/// velocity.
///
/// The draws are taken whether or not they are accepted, so a refused frame leaves none behind
/// for the next one.
///
/// # Errors
/// Fails before anything is declared or recorded: by name for temporal reconstruction,
/// screen-space passes, multisampling and any other pass the program runtime does not draw
/// into; with the signatures when the draws were prepared for other attachments; and when the
/// arena belongs to another authority or has retired.
pub(super) fn accept(
    gpu: &mut SceneGpu,
    encoder: &wgpu::CommandEncoder,
    renderer: &FrameRenderer,
    frame: (&ScenePipelines, bool),
) -> Result<Option<AdmittedPrograms>, FrameError> {
    let Some(programs) = gpu.take_programs() else {
        return Ok(None);
    };
    let (pipelines, writes_velocity) = frame;
    if writes_velocity || renderer.projection_jitter().is_some() {
        return Err(FrameError::UnsupportedEffect(
            "player programs with temporal reconstruction",
        ));
    }
    // Screen-space passes add attachments to the scene's pass declarations.
    if renderer.ssao.is_some() {
        return Err(FrameError::UnsupportedEffect(
            "player programs with screen-space passes",
        ));
    }
    let signature = TargetSignature::new(&[pipelines.color_format], Some(DEPTH), pipelines.samples)
        .map_err(|error| unsupported(&error))?;
    programs
        .check_target(signature)
        .and_then(|()| programs.retain(encoder))
        .map_err(|error| rejected(&error))?;
    Ok(Some(programs))
}

fn unsupported(error: &ProgramError) -> FrameError {
    match error {
        ProgramError::UnsupportedTarget(UnsupportedTarget::Multisampled { .. }) => {
            FrameError::UnsupportedEffect("player programs with multisampling")
        }
        ProgramError::UnsupportedTarget(UnsupportedTarget::ColorAttachments { .. }) => {
            FrameError::UnsupportedEffect("player programs with multiple render targets")
        }
        ProgramError::UnsupportedTarget(UnsupportedTarget::Stencil { .. }) => {
            FrameError::UnsupportedEffect("player programs with stencil")
        }
        other => rejected(other),
    }
}

fn rejected(error: &ProgramError) -> FrameError {
    FrameError::FeaturePreparation {
        feature: FEATURE,
        reason: error.to_string(),
    }
}
