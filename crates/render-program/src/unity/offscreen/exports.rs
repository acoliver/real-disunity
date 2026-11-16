//! The textures a frame's operations leave for the scene that follows them.
//!
//! The frame's registry does not know the names a frame plan reads by, so the host names each
//! texture it exports. An export is checked against what the operations wrote: a cell no
//! operation wrote has nothing for a scene pass to read.

use worldrunner_render_post::{OperationsOutput, ResourceRef};

use crate::ids::TextureId;

use super::OffscreenOperations;
use super::error::OffscreenError;

impl OffscreenOperations {
    /// Names one mip and layer of a texture of the frame, which the operations write, as
    /// `resource` in the frame plan. The admitted work reports it as an output
    /// (`FrameOperations::produces`), so a scene pass the plan places after the work may sample it.
    ///
    /// # Errors
    /// Fails when the texture is not one the frame uses, has no such mip and layer, or the
    /// operations never write it, when `resource` names another mip or layer, and when
    /// `resource` is already exported.
    pub fn export(
        &mut self,
        texture: TextureId,
        mip: u32,
        layer: u32,
        resource: ResourceRef,
    ) -> Result<(), OffscreenError> {
        let imported = self
            .tracker
            .imported(texture)
            .ok_or(OffscreenError::UnknownTexture(texture))?;
        if mip >= imported.mips || layer >= imported.layers {
            return Err(OffscreenError::NoSuchSubresource {
                texture,
                mip,
                element: layer,
            });
        }
        // A pass samples the mip and layer its resource names, so the name must be the cell
        // the export checked; otherwise validation accepts a read of a cell nothing wrote.
        if (resource.mip, resource.layer) != (mip, layer) {
            return Err(OffscreenError::ExportNamesAnotherSubresource {
                resource,
                mip,
                element: layer,
            });
        }
        if self.tracker.cell(imported.primary, mip, layer) == 0 {
            return Err(OffscreenError::NotWritten {
                texture,
                mip,
                element: layer,
            });
        }
        if self
            .exports
            .iter()
            .any(|output| output.resource == resource)
        {
            return Err(OffscreenError::ExportedTwice(resource));
        }
        self.exports.push(OperationsOutput::new(
            resource,
            imported.format,
            imported.size.map(|side| (side >> mip).max(1)),
        ));
        Ok(())
    }
}
