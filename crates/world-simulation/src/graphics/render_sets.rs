//! The renderers a camera render or a culling draws, fixed when the script calls it.
//!
//! `Camera.Render` draws at once, so a script may switch an object on for the call and off again
//! after it, as a bake into a render texture does. The frame's snapshot is taken when the frame
//! ends, when that object is inactive again. The world therefore records, for each camera render,
//! which renderers were drawable at the call, and the snapshot lists exactly those renderers. A
//! renderer that is drawable when the snapshot is taken but was not when a record was made is in no
//! set, so no record draws it.
//!
//! A set holds the mesh renderers that are enabled, not forced off, have a drawn mesh and sit on an
//! object active in the hierarchy, on a layer the camera's culling mask admits. Consecutive records
//! of one mask share a set while nothing that decides drawability changes; `drawable_epoch` counts
//! those changes.
//!
//! Skinned mesh renderers are not drawn yet, so no set holds one. The frame keeps the ones that
//! meet every other condition of a set, each once, so that its snapshot can say how many it leaves
//! out.
//!
//! Changes that follow from one already noted need no note of their own. A renderer's removal
//! retires its components first, which note it, so removing the object notes nothing more, and a
//! rolled back edit undoes a change that noted itself when it was made, with no record between.

use std::collections::{BTreeMap, BTreeSet};

use super::OffscreenWork;
use crate::{
    ComponentDesc, ComponentHandle, MeshFilter, MeshRenderer, ObjectHandle, RendererKind, World,
};

/// What a component contributes to a renderer being drawable. A component that contributes
/// nothing is not any set's concern, whether it is absent or only present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DrawInput {
    /// A mesh renderer of the mesh kind that is enabled and not forced off.
    ShownRenderer,
    /// A skinned mesh renderer that is enabled and not forced off. It is counted, not drawn.
    ShownSkinned,
    /// A mesh filter that has a drawn mesh. Which mesh it is does not matter to a set: the mesh
    /// is read when the snapshot is taken.
    MeshedFilter,
}

impl ComponentDesc {
    /// What this component contributes to deciding which renderers draw, or `None` when it
    /// contributes nothing: a mesh renderer that is enabled and not forced off (of the mesh or the
    /// skinned kind), or a mesh filter that has a drawn mesh. A component that contributes
    /// nothing is no set's concern, whether it is absent or only present.
    pub(crate) fn draw_input(&self) -> Option<DrawInput> {
        match self {
            Self::MeshRenderer(renderer) => match renderer.kind {
                RendererKind::Mesh => shows(renderer).then_some(DrawInput::ShownRenderer),
                RendererKind::Skinned => switched_on(renderer).then_some(DrawInput::ShownSkinned),
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

/// Whether `renderer` is a mesh renderer that is enabled and not forced off.
fn shows(renderer: &MeshRenderer) -> bool {
    renderer.kind == RendererKind::Mesh && switched_on(renderer)
}

/// Whether `renderer` is enabled and not forced off, whatever its kind.
fn switched_on(renderer: &MeshRenderer) -> bool {
    renderer.switches.enabled && !renderer.switches.force_rendering_off
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
    /// The skinned renderers that some set of the frame would have held if skinned meshes were
    /// drawn.
    skinned: BTreeSet<ComponentHandle>,
}

/// What a frame's camera renders captured, drained for the snapshot.
pub(crate) struct CapturedSets {
    pub(crate) sets: Vec<Vec<ComponentHandle>>,
    pub(crate) records: BTreeMap<u32, u32>,
    /// How many distinct skinned renderers the sets leave out.
    pub(crate) skinned: usize,
}

impl RenderSets {
    pub(crate) const fn new() -> Self {
        Self {
            sets: Vec::new(),
            records: BTreeMap::new(),
            by_mask: BTreeMap::new(),
            epoch: 0,
            skinned: BTreeSet::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.sets.clear();
        self.records.clear();
        self.by_mask.clear();
        self.skinned.clear();
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
            skinned: std::mem::take(&mut state.skinned).len(),
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
        let skinned = self.skinned_renderers(mask);
        let state = &mut self.graphics.render_sets;
        state.skinned.extend(skinned);
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

    /// The skinned renderers that would draw now on a layer `mask` admits if skinned meshes were
    /// drawn.
    fn skinned_renderers(&self, mask: u32) -> Vec<ComponentHandle> {
        self.published_components()
            .filter_map(|component| {
                let ComponentDesc::MeshRenderer(renderer) = &component.desc else {
                    return None;
                };
                (renderer.kind == RendererKind::Skinned
                    && switched_on(renderer)
                    && self.admits(mask, component.object))
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

    /// The identity of the mesh `renderer` draws when it is switched on, or `None` when it is not a
    /// mesh renderer that is enabled and not forced off, or its object has no drawn mesh. Whether
    /// the object is active is a separate question.
    fn drawn_mesh(&self, object: ObjectHandle, renderer: &MeshRenderer) -> Option<u64> {
        if shows(renderer) {
            self.filter_mesh(object)
        } else {
            None
        }
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
