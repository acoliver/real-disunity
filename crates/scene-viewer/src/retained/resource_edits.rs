//! Resource edits that change the resident scene in place: appended items and replaced
//! textures and meshes.
//!
//! The host validates and prepares an edit when it is accepted, changes its own scene at once
//! so later object edits see the new handles, and queues the prepared bytes for the renderer,
//! which admits and uploads only those bytes when the edit becomes the front generation.
use super::{RetainedHost, RetainedUpdate};
use crate::ViewerError;
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_scene::draw::{
    PreparedAppend, PreparedMeshReplacement, PreparedTextureReplacement, ResourceBase,
};

/// The upload form of one accepted resource edit.
#[derive(Debug)]
pub(crate) enum PreparedEdit {
    Append(PreparedAppend),
    Texture(PreparedTextureReplacement),
    Mesh(PreparedMeshReplacement),
}

impl PreparedEdit {
    /// The device storage the edit asks for, admitted before it uploads.
    pub(crate) fn allocations(&self) -> AllocationSet {
        match self {
            Self::Append(edit) => edit.allocations(),
            Self::Texture(edit) => edit.allocations(),
            Self::Mesh(edit) => edit.allocations(),
        }
    }

    /// The device features the prepared textures need.
    pub(crate) fn required_features(&self) -> wgpu::Features {
        match self {
            Self::Append(edit) => edit.required_features(),
            Self::Texture(edit) => edit.required_features(),
            Self::Mesh(_) => wgpu::Features::empty(),
        }
    }
}

pub(super) fn is_resource_edit(update: &RetainedUpdate) -> bool {
    matches!(
        update,
        RetainedUpdate::AppendResources(_)
            | RetainedUpdate::ReplaceTexture { .. }
            | RetainedUpdate::ReplaceMesh { .. }
    )
}

fn invalid(error: worldrunner_render_plan::SceneValidationError) -> ViewerError {
    ViewerError::Driver(error.to_string())
}

impl RetainedHost {
    /// Accepts an in-place resource edit, or fails before changing anything.
    pub(super) fn apply_resource_edit(
        &mut self,
        update: RetainedUpdate,
    ) -> Result<(), ViewerError> {
        if matches!(&update, RetainedUpdate::AppendResources(append) if append.is_empty()) {
            return Ok(());
        }
        let edit = self.prepare_edit(&update)?;
        self.edit_resources(&update)?;
        self.enqueue(update, false);
        self.readiness
            .queue
            .back_mut()
            .expect("accepted update")
            .edit = Some(edit);
        Ok(())
    }

    /// Prepares only the bytes the edit changes, which also refuses what needs a rebuild.
    fn prepare_edit(&self, update: &RetainedUpdate) -> Result<PreparedEdit, ViewerError> {
        let scene = self.scene();
        Ok(match update {
            RetainedUpdate::AppendResources(append) => {
                let base = ResourceBase {
                    textures: scene.resources().textures().len(),
                    materials: scene.resources().materials().len(),
                    meshes: scene.geometry().meshes().len(),
                };
                PreparedEdit::Append(PreparedAppend::new(base, append)?)
            }
            RetainedUpdate::ReplaceTexture { handle, texture } => {
                PreparedEdit::Texture(PreparedTextureReplacement::new(scene, *handle, texture)?)
            }
            RetainedUpdate::ReplaceMesh { handle, mesh } => {
                PreparedEdit::Mesh(PreparedMeshReplacement::new(scene, *handle, mesh)?)
            }
            _ => unreachable!("only resource edits are prepared"),
        })
    }

    /// Changes the host's scene, which stays what a full replacement would build.
    fn edit_resources(&mut self, update: &RetainedUpdate) -> Result<(), ViewerError> {
        let scene = self.scene.as_mut().expect("scene resolved before edits");
        match update {
            RetainedUpdate::AppendResources(append) => scene.append_resources(append.clone()),
            RetainedUpdate::ReplaceTexture { handle, texture } => {
                scene.replace_texture(*handle, texture.clone())
            }
            RetainedUpdate::ReplaceMesh { handle, mesh } => {
                scene.replace_mesh(*handle, mesh.clone())
            }
            _ => unreachable!("only resource edits change resources"),
        }
        .map_err(invalid)
    }
}

#[cfg(test)]
#[path = "resource_edits_tests.rs"]
pub(crate) mod tests;
