//! The work a scriptable render context records: culling results, and the handles later records
//! of the frame name them by. A handle is an id the script's side issues, nonzero and unused by
//! any record of the frame; the world remembers which kind of record issued it and forgets it
//! when the frame's work is taken or dropped.
use super::{GraphicsError, OffscreenWork};
use crate::{ComponentDesc, ComponentHandle, World, WorldError};

/// Most planes one culling result holds.
pub const MAX_CULL_PLANES: usize = 10;

/// What a handle of the frame names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HandleKind {
    Cull,
}

impl World {
    /// Checks a context record; other work is the offscreen list's own to check.
    pub(super) fn check_context(&self, work: &OffscreenWork) -> Result<(), WorldError> {
        match work {
            OffscreenWork::Cull {
                id, camera, planes, ..
            } => {
                self.check_new_handle(*id)?;
                self.check_camera(*camera)?;
                if planes.len() > MAX_CULL_PLANES {
                    return Err(GraphicsError::InvalidOffscreen.into());
                }
                if planes.iter().flatten().any(|value| !value.is_finite()) {
                    return Err(GraphicsError::NotFinite.into());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Remembers the handle a record issues.
    pub(super) fn note_handle(&mut self, work: &OffscreenWork) {
        if let OffscreenWork::Cull { id, .. } = work {
            self.graphics.handles.insert(*id, HandleKind::Cull);
        }
    }

    fn check_new_handle(&self, id: u64) -> Result<(), WorldError> {
        if id == 0 || self.graphics.handles.contains_key(&id) {
            Err(GraphicsError::InvalidOffscreen.into())
        } else {
            Ok(())
        }
    }

    fn check_camera(&self, camera: ComponentHandle) -> Result<(), WorldError> {
        if matches!(self.component(camera)?.desc, ComponentDesc::Camera(_)) {
            Ok(())
        } else {
            Err(GraphicsError::NotCamera.into())
        }
    }
}
