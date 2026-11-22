//! The UI overlay as staged, and the host's render textures it was staged with.
//!
//! The player offers the device texture of each render texture the UI reads. The overlay binds
//! them when it is staged, so offering a different texture for an id, or none, stages it again;
//! an offer that changes nothing the overlay reads costs nothing.
use super::{GpuContext, Presentation, Resident};
use crate::ViewerError;
use std::sync::Arc;
use worldrunner_render_scene::simulation::{UiOverlay, UiTarget};

#[derive(Default)]
pub(super) struct UiStaging {
    overlay: Option<Arc<UiOverlay>>,
    offered: Vec<UiTarget>,
}

impl UiStaging {
    /// Stages `overlay` with the textures offered now.
    pub(super) fn stage(
        &mut self,
        context: &GpuContext,
        (resident, presentation): (&mut Resident, &mut Presentation),
        overlay: &Arc<UiOverlay>,
    ) -> Result<(), ViewerError> {
        super::retained::apply_ui(context, resident, presentation, overlay, &self.offered)?;
        self.overlay = Some(Arc::clone(overlay));
        Ok(())
    }

    /// Takes `targets` as the textures on offer. The staged overlay is staged again when one of
    /// the textures it reads is now another or gone, which also lets go of the old one.
    pub(super) fn offer(
        &mut self,
        context: &GpuContext,
        (resident, presentation): (&mut Resident, &mut Presentation),
        targets: &[UiTarget],
    ) -> Result<(), ViewerError> {
        let before = std::mem::replace(&mut self.offered, targets.to_vec());
        let Some(overlay) = self.overlay.clone() else {
            return Ok(());
        };
        let read = |offered: &[UiTarget]| -> Vec<Option<UiTarget>> {
            overlay
                .targets()
                .iter()
                .map(|id| offered.iter().find(|target| target.id() == *id).cloned())
                .collect()
        };
        if read(&before) == read(targets) {
            return Ok(());
        }
        super::retained::apply_ui(context, resident, presentation, &overlay, &self.offered)
    }
}
