//! Publishes an accepted resource edit: admits its bytes, then uploads only those bytes.
use super::{GpuContext, Presentation, Resident};
use crate::retained::resource_edits::PreparedEdit;
use crate::{RetainedUpdate, ViewerError};
use worldrunner_render_scene::draw::SceneGpu;
use worldrunner_render_scene::renderer::ScenePipelines;

impl Resident {
    /// The resident scene with the pipelines its material bind groups are built against.
    fn scene_and_pipelines_mut(&mut self) -> (&mut SceneGpu, &ScenePipelines) {
        match self {
            Self::Plain(plain) => (&mut plain.gpu, &plain.pipelines),
            Self::Shadowed(shadowed) => shadowed.scene_and_pipelines_mut(),
        }
    }
}

/// Reserves the edit's storage against the presentation budget, then uploads it. Nothing is
/// uploaded when the device lacks a feature the new textures need or the budget refuses the
/// bytes, and the resident scene is unchanged when either fails.
pub(super) fn publish(
    context: &GpuContext,
    resident: &mut Resident,
    presentation: &mut Presentation,
    edit: PreparedEdit,
    update: &RetainedUpdate,
) -> Result<(), ViewerError> {
    if !context
        .device()
        .features()
        .contains(edit.required_features())
    {
        return Err(ViewerError::UnsupportedScene(
            "streamed textures require unavailable device features",
        ));
    }
    let reservation = presentation
        .scheduler
        .reserve(context, edit.allocations())?;
    let (gpu, pipelines) = resident.scene_and_pipelines_mut();
    let (device, authority) = (context.device(), context.submission());
    match (edit, update) {
        (PreparedEdit::Append(prepared), RetainedUpdate::AppendResources(source)) => {
            gpu.append_prepared(device, authority, pipelines, prepared, source)?;
        }
        (PreparedEdit::Texture(replacement), _) => {
            gpu.replace_texture_prepared(device, authority, pipelines, &replacement)?;
        }
        (PreparedEdit::Mesh(replacement), _) => {
            gpu.replace_mesh_prepared(device, authority.queue(), replacement)?;
        }
        (PreparedEdit::Append(_), _) => {
            return Err(ViewerError::Driver(
                "an appended resource edit lost its update".into(),
            ));
        }
    }
    gpu.retain_append_reservation(reservation);
    Ok(())
}
