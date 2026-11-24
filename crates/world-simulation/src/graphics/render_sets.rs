//! The renderers a camera render or a culling draws, fixed when the script calls it.
//!
//! `Camera.Render` draws at once, so a script may switch an object on for the call and off again
//! after it, as a bake into a render texture does. The frame's snapshot is taken when the frame
//! ends, when that object is inactive again. The world therefore records, for each camera render,
//! which renderers were drawable at the call, and the snapshot lists exactly those renderers. A
//! renderer that is drawable when the snapshot is taken but was not when a record was made is in no
//! set, so no record draws it.
//!
//! A set holds the mesh and skinned renderers that are enabled, not forced off, have a drawn mesh and
//! sit on an object active in the hierarchy, on a layer the camera's culling mask admits. Consecutive records
//! of one mask share a set while nothing that decides drawability changes; `drawable_epoch` counts
//! those changes.
//!
//! Changes that follow from one already noted need no note of their own. A renderer's removal
//! retires its components first, which note it, so removing the object notes nothing more, and a
//! rolled back edit undoes a change that noted itself when it was made, with no record between.

use std::collections::BTreeMap;

use glam::Mat4;

use super::OffscreenWork;
use crate::{
    ComponentDesc, ComponentHandle, MeshFilter, MeshRenderer, ObjectHandle, RendererKind, World,
    WorldError,
};

/// What a component contributes to a renderer being drawable. A component that contributes
/// nothing is not any set's concern, whether it is absent or only present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DrawInput {
    /// A mesh renderer of the mesh kind that is enabled and not forced off.
    ShownRenderer,
    /// A skinned mesh renderer that is enabled, not forced off and names a drawn mesh.
    ShownSkinned,
    /// A mesh filter that has a drawn mesh. Which mesh it is does not matter to a set: the mesh
    /// is read when the snapshot is taken.
    MeshedFilter,
}

impl ComponentDesc {
    /// What this component contributes to deciding which renderers draw, or `None` when it
    /// contributes nothing: a mesh renderer that is enabled and not forced off, a skinned one
    /// only when it names a drawn mesh, or a mesh filter that has a drawn mesh. A component that
    /// contributes nothing is no set's concern, whether it is absent or only present.
    pub(crate) fn draw_input(&self) -> Option<DrawInput> {
        match self {
            Self::MeshRenderer(renderer) if !renderer.shows() => None,
            Self::MeshRenderer(renderer) => match renderer.kind {
                RendererKind::Mesh => Some(DrawInput::ShownRenderer),
                RendererKind::Skinned => renderer.own_mesh().map(|_| DrawInput::ShownSkinned),
                RendererKind::Particle => None,
            },
            Self::MeshFilter(filter) => filter_draws(filter).map(|_| DrawInput::MeshedFilter),
            _ => None,
        }
    }
}

/// The 32 bit index of the `count`th record, set or renderer of a frame's snapshot. A frame holds
/// fewer than [`MAX_OFFSCREEN_WORK`](super::MAX_OFFSCREEN_WORK) records, which `push_offscreen`
/// enforces before it numbers one, and no more sets than records. A snapshot lists a renderer at
/// most once, and the world holds at most `max_components` components, a `u32`.
pub(super) fn index_of(count: usize) -> u32 {
    u32::try_from(count).expect("a frame's records, sets and renderers are fewer than 2^32")
}

/// The mesh a mesh filter draws, or `None` when it has none. Which mesh it is does not matter to
/// a set, which holds renderers: the mesh is read when the snapshot is taken.
fn filter_draws(filter: &MeshFilter) -> Option<u64> {
    (filter.drawn_mesh != 0).then_some(filter.drawn_mesh)
}

/// The sets the frame's camera renders drew with, and which record drew with which.
#[derive(Clone)]
pub(crate) struct RenderSets {
    /// The distinct sets, in the order the frame first needed them.
    sets: Vec<Vec<ComponentHandle>>,
    /// The set of the camera render at each index of the frame's work.
    records: BTreeMap<u32, u32>,
    /// The sets made since the drawable renderers last changed, by culling mask.
    by_mask: BTreeMap<u32, u32>,
    /// The `drawable_epoch` that `by_mask` belongs to.
    epoch: u64,
}

/// What a frame's camera renders captured, drained for the snapshot.
pub(crate) struct CapturedSets {
    pub(crate) sets: Vec<Vec<ComponentHandle>>,
    pub(crate) records: BTreeMap<u32, u32>,
}

impl RenderSets {
    pub(crate) const fn new() -> Self {
        Self {
            sets: Vec::new(),
            records: BTreeMap::new(),
            by_mask: BTreeMap::new(),
            epoch: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.sets.clear();
        self.records.clear();
        self.by_mask.clear();
    }
}

impl World {
    /// Notes that a change may have altered which renderers are drawable: an object's activity,
    /// parent or layer, or a component that decides drawing (see
    /// [`ComponentDesc::draw_input`]).
    pub(crate) fn note_drawables_changed(&mut self) {
        self.graphics.drawable_epoch += 1;
    }

    /// Notes a component going from the draw input `before` to `after` (see
    /// [`ComponentDesc::draw_input`]), where a component that was not there or is not there now
    /// contributes none. Only a change of draw input alters the drawable renderers, and that
    /// includes a renderer replaced by one of another kind, which a flag could not tell apart.
    pub(crate) fn note_drawable_edit(
        &mut self,
        before: Option<DrawInput>,
        after: Option<DrawInput>,
    ) {
        if before != after {
            self.note_drawables_changed();
        }
    }

    /// Takes what the frame's camera renders captured. [`World::take_offscreen`], which a take
    /// follows, drops the sets by mask along with the rest of the frame.
    pub(crate) fn take_render_sets(&mut self) -> CapturedSets {
        let state = &mut self.graphics.render_sets;
        CapturedSets {
            sets: std::mem::take(&mut state.sets),
            records: std::mem::take(&mut state.records),
        }
    }

    /// Fixes the renderers the camera render or culling about to be recorded at index `record` of
    /// the frame's work draws. A camera render uses its camera's culling mask and a culling its
    /// own. A camera render whose camera is not readable keeps no set, and the snapshot gives it no
    /// camera to draw with.
    pub(super) fn capture_render_set(&mut self, work: &OffscreenWork, record: usize) {
        let mask = match work {
            OffscreenWork::CameraRender { camera, .. } => {
                match self.component(*camera).map(|held| &held.desc) {
                    Ok(ComponentDesc::Camera(settings)) => settings.culling_mask,
                    _ => return,
                }
            }
            OffscreenWork::Cull { culling_mask, .. } => *culling_mask,
            _ => return,
        };
        let set = self.set_for(mask);
        self.graphics
            .render_sets
            .records
            .insert(index_of(record), set);
    }

    /// The index of the set for `mask`: the one made since the drawable renderers last changed,
    /// or a new one.
    fn set_for(&mut self, mask: u32) -> u32 {
        if self.graphics.render_sets.epoch != self.graphics.drawable_epoch {
            self.graphics.render_sets.epoch = self.graphics.drawable_epoch;
            self.graphics.render_sets.by_mask.clear();
        }
        if let Some(set) = self.graphics.render_sets.by_mask.get(&mask) {
            return *set;
        }
        let drawable = self.drawable_renderers(mask);
        let state = &mut self.graphics.render_sets;
        let set = index_of(state.sets.len());
        state.sets.push(drawable);
        state.by_mask.insert(mask, set);
        set
    }

    /// The mesh renderers that draw now on a layer `mask` admits.
    fn drawable_renderers(&self, mask: u32) -> Vec<ComponentHandle> {
        self.published_components()
            .filter_map(|component| {
                let ComponentDesc::MeshRenderer(renderer) = &component.desc else {
                    return None;
                };
                (self.admits(mask, component.object)
                    && self.drawn_mesh(component.object, renderer).is_some())
                .then_some(component.handle)
            })
            .collect()
    }

    /// Whether `object` is active in the hierarchy on a layer `mask` admits.
    fn admits(&self, mask: u32, object: ObjectHandle) -> bool {
        let Ok(layer) = self.object_layer(object) else {
            return false;
        };
        mask.checked_shr(u32::from(layer))
            .is_some_and(|bits| bits & 1 == 1)
            && self.active_in_hierarchy(object).unwrap_or(false)
    }

    /// The identity of the mesh `renderer` draws when it is switched on, or `None` when it is not
    /// enabled and not forced off, or has no drawn mesh. Whether the object is active is a separate
    /// question.
    fn drawn_mesh(&self, object: ObjectHandle, renderer: &MeshRenderer) -> Option<u64> {
        if renderer.shows() {
            self.renderer_mesh(object, renderer)
        } else {
            None
        }
    }

    /// The identity of the mesh `renderer` on `object` names, whatever its switches say: a mesh
    /// renderer's is its object's mesh filter's, a skinned renderer's its own, and a particle
    /// renderer has none. Every list of drawn renderers asks here, so they agree.
    pub(crate) fn renderer_mesh(
        &self,
        object: ObjectHandle,
        renderer: &MeshRenderer,
    ) -> Option<u64> {
        match renderer.kind {
            RendererKind::Mesh => self.filter_mesh(object),
            RendererKind::Skinned => renderer.own_mesh(),
            RendererKind::Particle => None,
        }
    }

    /// The matrix that places the mesh of `renderer` on `object`. A mesh renderer's mesh is in its
    /// own object's space. A skinned renderer is drawn in its bind pose, where every bone's matrix
    /// times its bind pose is the matrix the mesh was bound at, so the root bone's matrix times the
    /// root bone's bind pose places it. `None` when the skin names no root bone or the root bone is
    /// gone: the skin then does not say where the mesh goes, and the renderer is left out.
    ///
    /// # Errors
    /// Fails when the world cannot describe the object of a mesh renderer.
    pub(crate) fn renderer_placement(
        &self,
        object: ObjectHandle,
        renderer: &MeshRenderer,
    ) -> Result<Option<Mat4>, WorldError> {
        if renderer.kind != RendererKind::Skinned {
            return self.presented_matrix(object).map(Some);
        }
        Ok(renderer.skin.root.and_then(|root| {
            let bone = self.presented_matrix(root.bone).ok()?;
            Some(bone * Mat4::from_cols_array(&root.bind_pose))
        }))
    }

    /// The drawn mesh of the object's mesh filter, when it has one with a mesh.
    pub(crate) fn filter_mesh(&self, object: ObjectHandle) -> Option<u64> {
        let stored = self.live_object(object).ok()?;
        stored
            .components
            .iter()
            .rev()
            .filter_map(|handle| self.components.get(handle.0))
            .find_map(|component| match &component.desc {
                ComponentDesc::MeshFilter(filter) => filter_draws(filter),
                _ => None,
            })
    }
}
