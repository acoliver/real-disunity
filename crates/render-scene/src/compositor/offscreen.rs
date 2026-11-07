//! Offscreen operations of the player, encoded before the scene's own.
//!
//! The scene feature draws into the frame's color and depth targets. Offscreen work draws into
//! render textures the runtime owns, so it needs none of the scene's attachments and runs first,
//! in the same encoder and so the same submission. It is encoded through the post chain's pass
//! timer, which lists each operation in the frame's timing as a [`PassId::Operation`].
//!
//! [`PassId::Operation`]: worldrunner_render_post::PassId::Operation

use super::FrameRenderer;
use crate::draw::SceneGpu;
use crate::renderer::ScenePipelines;
use worldrunner_render_post::FrameError;
use worldrunner_render_program::runtime::AdmittedPrograms;

const FEATURE: &str = "offscreen operations";

/// Takes the player's work admitted to `gpu`: encodes its offscreen operations, if any, into
/// `encoder`, and returns the draws that join the opaque pass. `frame` is the scene pipelines and
/// whether the frame writes velocity.
///
/// Both are taken whether or not they are accepted, so a refused frame leaves none behind for the
/// next. The draws are checked first, because they name the frame's attachments.
///
/// # Errors
/// As [`super::programs::accept`]; and by name when the arena of the offscreen work retired or
/// the frame belongs to another authority, and with the contract's error when a resource was
/// replaced since admission.
pub(super) fn encode_player_work(
    gpu: &mut SceneGpu,
    encoder: &mut wgpu::CommandEncoder,
    renderer: &FrameRenderer,
    frame: (&ScenePipelines, bool),
) -> Result<Option<AdmittedPrograms>, FrameError> {
    let offscreen = gpu.take_offscreen();
    let programs = super::programs::accept(gpu, encoder, renderer, frame)?;
    if let Some(offscreen) = offscreen {
        offscreen
            .retain(encoder)
            .map_err(|error| FrameError::FeaturePreparation {
                feature: FEATURE,
                reason: error.to_string(),
            })?;
        offscreen.encode(Some(&renderer.chain.timing_state()), encoder)?;
    }
    Ok(programs)
}
