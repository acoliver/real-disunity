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
pub(super) struct Key {
    edits: u64,
    matrix: [u32; 16],
    eye: [i64; 3],
    fade_eye: [u32; 3],
    casters_only: bool,
    /// Generation of the cascade's slice hull; zero without one.
    slice: u64,
}

impl Key {
    fn new(edits: u64, matrix: glam::Mat4, frame: &FrameUniform, casters_only: bool) -> Self {
        Self {
            edits,
            matrix: matrix.to_cols_array().map(f32::to_bits),
            eye: frame.camera_position_millimeters,
            fade_eye: frame.camera_position.to_array().map(f32::to_bits),
            casters_only,
            slice: 0,
        }
    }
}

/// The selection a view last published and the one it last built.
#[derive(Debug, Default)]
pub(super) struct View {
    pub published: Rc<RefCell<Rc<Selection>>>,
    built: RefCell<Option<(Key, Rc<Selection>)>>,
    pub compaction: super::compaction::Compaction,
    pub slice: super::slice_hull::CascadeSlice,
    #[cfg(test)]
    pub rebuilds: std::cell::Cell<usize>,
}

/// Instance edit count and receiver matrix bits.
type CasterKey = (u64, [u32; 16]);

/// Recent caster fits, at most one per shadow cascade.
#[derive(Debug, Default)]
pub(super) struct CasterFits {
    fits: RefCell<VecDeque<(CasterKey, glam::Mat4)>>,
    scratch: RefCell<super::visibility::FitScratch>,
    #[cfg(test)]
    pub computed: std::cell::Cell<usize>,
}

pub(super) fn view_index(shadow_matrix: Option<(u32, glam::Mat4)>) -> usize {
    shadow_matrix.map_or(CAMERA, |(index, _)| index as usize + 1)
}

impl SceneGpu {
    fn view_matrix(shadow_matrix: Option<(u32, glam::Mat4)>, frame: &FrameUniform) -> glam::Mat4 {
        shadow_matrix.map_or(frame.selection_view_projection(), |(_, matrix)| matrix)
    }

    /// Identifies the inputs of the selection `select_objects` returns for the same arguments.
    pub(super) fn selection_key(
        &self,
        shadow_matrix: Option<(u32, glam::Mat4)>,
        frame: &FrameUniform,
    ) -> Key {
        self.selection_key_at(self.instances.edits(), shadow_matrix, frame)
    }

    /// `selection_key` for a caller that already read the placements' edit count.
    pub(super) fn selection_key_at(
        &self,
        edits: u64,
        shadow_matrix: Option<(u32, glam::Mat4)>,
        frame: &FrameUniform,
    ) -> Key {
        let mut key = Key::new(
            edits,
            Self::view_matrix(shadow_matrix, frame),
            frame,
            shadow_matrix.is_some(),
        );
        key.slice = self.slice_generation(shadow_matrix);
        key
    }

    /// The generation of the slice hull `view_slice` returns, which is all a key needs of it.
    fn slice_generation(&self, shadow_matrix: Option<(u32, glam::Mat4)>) -> u64 {
        let Some((_, matrix)) = shadow_matrix else {
            return 0;
        };
        #[cfg(test)]
        if super::slice_hull::UNCULLED.get() {
            return 0;
        }
        self.views[view_index(shadow_matrix)]
            .slice
            .generation_for(matrix)
            .unwrap_or(0)
    }

    fn view_slice(
        &self,
        shadow_matrix: Option<(u32, glam::Mat4)>,
    ) -> Option<(super::slice_hull::SliceHull, u64)> {
        let (_, matrix) = shadow_matrix?;
        #[cfg(test)]
        if super::slice_hull::UNCULLED.get() {
            return None;
        }
        self.views[view_index(shadow_matrix)]
            .slice
            .for_matrix(matrix)
    }

    /// Selects the camera view, or a cascade when `shadow_matrix` is given.
    pub(super) fn select_objects(
        &self,
        shadow_matrix: Option<(u32, glam::Mat4)>,
        frame: &FrameUniform,
    ) -> Rc<Selection> {
        self.select_views(&[shadow_matrix], frame);
        self.views[view_index(shadow_matrix)]
            .built
            .borrow()
            .as_ref()
            .map(|(_, selection)| Rc::clone(selection))
            .expect("a selected view")
    }

    /// Builds the selection of every view in `views` whose inputs changed since it was last
    /// built, on as many of `selection_threads` as the work is worth. The views read the same
    /// placements and fill a shared memo of each placement's level for the frame's eye, and each
    /// writes only its own selection, so their results do not depend on one another or on the
    /// order they run in.
    pub(super) fn select_views(&self, views: &[Option<(u32, glam::Mat4)>], frame: &FrameUniform) {
        let mut jobs: Vec<(usize, Key, glam::Mat4, Selection)> = Vec::new();
        for &shadow_matrix in views {
            let index = view_index(shadow_matrix);
            let key = self.selection_key(shadow_matrix, frame);
            let view = &self.views[index];
            if jobs.iter().any(|(queued, ..)| *queued == index)
                || view
                    .built
                    .borrow()
                    .as_ref()
                    .is_some_and(|(built, _)| *built == key)
            {
                continue;
            }
            // Selecting resets everything but these, so a view starts from what it last held
            // only in capacity: the previous result may still be lent to a pass.
            let mut selection = Selection::with_capacity(
                view.built
                    .borrow()
                    .as_ref()
                    .map_or(0, |(_, built)| built.ranges.len()),
            );
            selection.casters_only = key.casters_only;
            selection.slice = self.view_slice(shadow_matrix).map(|(hull, _)| hull);
            jobs.push((
                index,
                key,
                Self::view_matrix(shadow_matrix, frame),
                selection,
            ));
        }
        if jobs.is_empty() {
            return;
        }
        super::visibility::with_placements(
            &self.instances,
            &self.mesh_selection,
            (frame.camera_position_millimeters, frame.camera_position),
            &self.placement_rows,
            |placements| {
                let mut views: Vec<_> = jobs
                    .iter_mut()
                    .map(|(_, _, matrix, selection)| (*matrix, selection))
                    .collect();
                let threads = placements.threads_for(views.len(), self.selection_threads);
                placements.select_each(&mut views, threads);
            },
        );
        for (index, key, _, selection) in jobs {
            let view = &self.views[index];
            #[cfg(test)]
            view.rebuilds.set(view.rebuilds.get() + 1);
            *view.built.borrow_mut() = Some((key, Rc::new(selection)));
        }
    }

    /// Replaces each receiver with its fit over the casters, as the cascades of a frame ask for
    /// them. Fits made for an unchanged receiver while no placement changed are reused, and the
    /// rest share one borrow of every batch's casters.
    pub(crate) fn caster_matrices(&self, receivers: &mut [glam::Mat4]) {
        let edits = self.instances.edits();
        let terrain = self.terrain.as_ref().map(|terrain| terrain.bounds);
        let mut fits = self.caster_fits.fits.borrow_mut();
        let mut scratch = self.caster_fits.scratch.borrow_mut();
        let mut scan =
            super::visibility::CasterScan::new(&self.instances, &self.mesh_selection, &mut scratch);
        for receiver in receivers {
            let key = (edits, receiver.to_cols_array().map(f32::to_bits));
            if let Some((_, fit)) = fits.iter().find(|(cached, _)| *cached == key) {
                *receiver = *fit;
                continue;
            }
            #[cfg(test)]
            self.caster_fits
                .computed
                .set(self.caster_fits.computed.get() + 1);
            let fit = scan.matrix(terrain, *receiver);
            if fits.len() == self.views.len() - 1 {
                fits.pop_front();
            }
            fits.push_back((key, fit));
            *receiver = fit;
        }
    }

    /// `caster_matrices` for one receiver.
    #[cfg(test)]
    pub(crate) fn caster_matrix(&self, receiver: glam::Mat4) -> glam::Mat4 {
        let mut fit = [receiver];
        self.caster_matrices(&mut fit);
        fit[0]
    }
}

#[cfg(test)]
mod key_tests;
