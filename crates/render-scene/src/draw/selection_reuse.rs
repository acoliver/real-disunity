//! Per-view object selections, reused while their inputs are unchanged.
//!
//! A selection depends on the placements, the fixed mesh and material tables, the view matrix,
//! the eye position and whether only casters count. Placements change only through batch edits,
//! which `Instances::edits` counts, so equal keys select the same ranges. Several passes share
//! the camera view each frame, and a still view repeats every frame.
use std::{cell::RefCell, rc::Rc};

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
}
