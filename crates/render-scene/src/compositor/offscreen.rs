//! Offscreen operations of the player, positioned in the frame's pass list.
//!
//! The scene feature draws into the frame's color and depth targets. Offscreen work draws into
//! render textures the runtime owns, so it needs none of the scene's attachments and runs ahead
//! of the scene's passes, in the same encoder and so the same submission. It belongs to the
//! frame plan as an operations group positioned before the first pass, which the post chain's
//! executor encodes through its pass timer, so each operation is listed in the frame's timing as
//! a [`PassId::Operation`].
//!
//! [`PassId::Operation`]: worldrunner_render_post::PassId::Operation

use super::FrameRenderer;
use crate::draw::SceneGpu;
use crate::renderer::ScenePipelines;
use worldrunner_render_post::FrameError;
use worldrunner_render_program::offscreen::AdmittedOffscreen;
use worldrunner_render_program::runtime::AdmittedPrograms;

const FEATURE: &str = "offscreen operations";

/// Takes the offscreen operations admitted to `gpu`, for a path that draws no player programs of
/// its own (the shadowed one), and retains their arena and storage on `encoder`. They are taken
/// whether or not the frame goes on to be accepted, so a refused frame leaves none behind.
///
/// # Errors
/// By name when the arena of the offscreen work retired or the frame belongs to another
/// authority.
pub(crate) fn take_offscreen(
    gpu: &mut SceneGpu,
    encoder: &wgpu::CommandEncoder,
) -> Result<Option<AdmittedOffscreen>, FrameError> {
    let offscreen = gpu.take_offscreen();
    if let Some(offscreen) = &offscreen {
        retain(offscreen, encoder)?;
    }
    Ok(offscreen)
}

fn retain(offscreen: &AdmittedOffscreen, encoder: &wgpu::CommandEncoder) -> Result<(), FrameError> {
    offscreen
        .retain(encoder)
        .map_err(|error| FrameError::FeaturePreparation {
            feature: FEATURE,
            reason: error.to_string(),
        })
}

/// Takes the player's work admitted to `gpu`. Returns the draws that join the opaque pass and the
/// offscreen operations, if any, for the frame plan to position. `frame` is the scene pipelines
/// and whether the frame writes velocity.
///
/// Both are taken whether or not they are accepted, so a refused frame leaves none behind for the
/// next. The draws are checked first, because they name the frame's attachments. The operations
/// retain their arena and storage on `encoder` here, before anything is encoded, and are encoded
/// later by the executor.
///
/// # Errors
/// As [`super::programs::accept`]; and by name when the arena of the offscreen work retired or
/// the frame belongs to another authority. A resource replaced since admission is the contract's
/// error when the executor encodes the operations.
pub(super) fn encode_player_work(
    gpu: &mut SceneGpu,
    encoder: &mut wgpu::CommandEncoder,
    renderer: &FrameRenderer,
    frame: (&ScenePipelines, bool),
) -> Result<(Option<AdmittedPrograms>, Option<AdmittedOffscreen>), FrameError> {
    let offscreen = gpu.take_offscreen();
    let programs = super::programs::accept(gpu, encoder, renderer, frame)?;
    if let Some(offscreen) = &offscreen {
        retain(offscreen, encoder)?;
    }
    Ok((programs, offscreen))
}
