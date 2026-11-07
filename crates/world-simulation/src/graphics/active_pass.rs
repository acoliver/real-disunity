//! The pass a material activated for the immediate-mode draws that follow.
use super::MaterialId;
use crate::{World, WorldError};

/// A pass of a material, activated for the immediate-mode draws that follow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivePass {
    pub material: MaterialId,
    pub pass: u32,
}

impl World {
    /// Activates `pass` of a live material, replacing the pass that was active. Whether the shader
    /// has such a pass is for the caller to check: the world holds no pass counts.
    pub fn set_active_pass(&mut self, material: MaterialId, pass: u32) -> Result<(), WorldError> {
        self.material(material)?;
        self.graphics.active_pass = Some(ActivePass { material, pass });
        Ok(())
    }

    /// The pass the last [`World::set_active_pass`] activated, or none when nothing did or the
    /// material it names was destroyed. Nothing draws with it yet.
    pub fn active_pass(&self) -> Option<ActivePass> {
        self.graphics.active_pass
    }

    pub(super) fn clear_active_pass_of(&mut self, material: MaterialId) {
        if self
            .graphics
            .active_pass
            .is_some_and(|held| held.material == material)
        {
            self.graphics.active_pass = None;
        }
    }
}
