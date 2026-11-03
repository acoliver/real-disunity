//! Per-view object selections and shadow caster fits, reused while their inputs are unchanged.
//!
//! A selection depends on the placements, the fixed mesh and material tables, the view matrix,
//! the eye position and whether only casters count. Placements change only through batch edits,
//! which `Instances::edits` counts, so equal keys select the same ranges. Several passes share
//! the camera view each frame, and a still view repeats every frame. Caster fits depend on the
//! placements, the terrain bounds (fixed after upload) and the receiver matrix.
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use super::visibility::Selection;
use super::{FrameUniform, SceneGpu};

/// Index of the camera in `SceneGpu::views`; shadow cascade `n` is at `n + 1`.
pub(super) const CAMERA: usize = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    edits: u64,
    matrix: [u32; 16],
    eye: [i64; 3],
    casters_only: bool,
}

/// The selection a view last published and the one it last built.
#[derive(Debug, Default)]
pub(super) struct View {
    pub published: Rc<RefCell<Selection>>,
    built: RefCell<Option<(Key, Selection)>>,
    #[cfg(test)]
    pub rebuilds: std::cell::Cell<usize>,
}

/// Instance edit count and receiver matrix bits.
type CasterKey = (u64, [u32; 16]);

/// Recent caster fits, at most one per shadow cascade.
#[derive(Debug, Default)]
pub(super) struct CasterFits {
    fits: RefCell<VecDeque<(CasterKey, glam::Mat4)>>,
    #[cfg(test)]
    pub computed: std::cell::Cell<usize>,
}

pub(super) fn view_index(shadow_matrix: Option<(u32, glam::Mat4)>) -> usize {
    shadow_matrix.map_or(CAMERA, |(index, _)| index as usize + 1)
}

impl SceneGpu {
    /// Selects the camera view, or a cascade when `shadow_matrix` is given.
    pub(super) fn select_objects(
        &self,
        shadow_matrix: Option<(u32, glam::Mat4)>,
        frame: &FrameUniform,
    ) -> Selection {
        let matrix = shadow_matrix.map_or(frame.selection_view_projection(), |(_, matrix)| matrix);
        let key = Key {
            edits: self.instances.edits(),
            matrix: matrix.to_cols_array().map(f32::to_bits),
            eye: frame.camera_position_millimeters,
            casters_only: shadow_matrix.is_some(),
        };
        let view = &self.views[view_index(shadow_matrix)];
        let mut built = view.built.borrow_mut();
        if let Some((_, selection)) = built.as_ref().filter(|(built, _)| *built == key) {
            return selection.clone();
        }
        #[cfg(test)]
        view.rebuilds.set(view.rebuilds.get() + 1);
        let mut selection = view.published.borrow().clone();
        selection.casters_only = key.casters_only;
        selection.rebuild(&self.instances, &self.mesh_selection, matrix, key.eye);
        *built = Some((key, selection.clone()));
        selection
    }

    /// Extends the receiver's depth range over the casters, reusing the fit for an unchanged
    /// receiver while no placement changed.
    pub(crate) fn caster_matrix(&self, receiver: glam::Mat4) -> glam::Mat4 {
        let key = (
            self.instances.edits(),
            receiver.to_cols_array().map(f32::to_bits),
        );
        let mut fits = self.caster_fits.fits.borrow_mut();
        if let Some((_, fit)) = fits.iter().find(|(cached, _)| *cached == key) {
            return *fit;
        }
        #[cfg(test)]
        self.caster_fits
            .computed
            .set(self.caster_fits.computed.get() + 1);
        let fit = super::visibility::caster_matrix(
            &self.instances,
            &self.mesh_selection,
            self.terrain.as_ref().map(|terrain| terrain.bounds),
            receiver,
        );
        if fits.len() == self.views.len() - 1 {
            fits.pop_front();
        }
        fits.push_back((key, fit));
        fit
    }
}
