//! The renderers a camera render or a culling draws, fixed when the script calls it.
//!
//! `Camera.Render` draws at once, so a script may switch an object on for the call and off again
//! after it, as a bake into a render texture does. The frame's snapshot is taken when the frame
//! ends, when that object is inactive again. The world therefore records, for each camera render,
//! which renderers were drawable at the call, and the snapshot lists them next to the renderers
//! that are drawable when it is taken.
//!
//! A set holds the mesh renderers that are enabled, not forced off, have a drawn mesh and sit on an
//! object active in the hierarchy, on a layer the camera's culling mask admits. Consecutive records
//! of one mask share a set while nothing that decides drawability changes; `drawable_epoch` counts
//! those changes.

use std::collections::BTreeMap;

use super::OffscreenWork;
use crate::{
    ComponentDesc, ComponentHandle, ComponentKind, MeshRenderer, ObjectHandle, RendererKind, World,
};

/// The sets the frame's camera renders drew with, and which record drew with which.
#[derive(Clone, Default)]
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
#[derive(Default)]
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
    /// Notes that a change may have altered which renderers are drawable: an object's activity or
    /// parent, a renderer's switches, a mesh filter, a layer, or the existence of any of them.
    pub(crate) fn note_drawables_changed(&mut self) {
        self.graphics.drawable_epoch += 1;
    }

    /// Notes a change to a component of `kind`, which alters the drawable renderers when it is a
    /// mesh renderer or a mesh filter.
    pub(crate) fn note_component_drawables(&mut self, kind: ComponentKind) {
        if matches!(
            kind,
            ComponentKind::MeshRenderer | ComponentKind::MeshFilter
        ) {
            self.note_drawables_changed();
        }
    }

    /// Takes what the frame's camera renders captured.
    pub(crate) fn take_render_sets(&mut self) -> CapturedSets {
        let state = &mut self.graphics.render_sets;
        state.by_mask.clear();
        CapturedSets {
            sets: std::mem::take(&mut state.sets),
            records: std::mem::take(&mut state.records),
        }
    }

    /// Fixes the renderers the camera render or culling about to be recorded draws. A camera
    /// render uses its camera's culling mask and a culling its own. A camera render whose camera is
    /// not readable keeps no set and draws what the snapshot lists.
    pub(super) fn capture_render_set(&mut self, work: &OffscreenWork) {
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
        let Ok(record) = u32::try_from(self.graphics.offscreen.len()) else {
            return;
        };
        let set = self.set_for(mask);
        self.graphics.render_sets.records.insert(record, set);
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
        let set = u32::try_from(state.sets.len()).unwrap_or(u32::MAX);
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
                let layer = self.object_layer(component.object).ok()?;
                let admitted = mask
                    .checked_shr(u32::from(layer))
                    .is_some_and(|bits| bits & 1 == 1);
                (admitted
                    && self.drawn_mesh(component.object, renderer).is_some()
                    && self.active_in_hierarchy(component.object).ok()?)
                .then_some(component.handle)
            })
            .collect()
    }

    /// The identity of the mesh `renderer` draws when it is switched on, or `None` when it is not a
    /// mesh renderer that is enabled and not forced off, or its object has no drawn mesh. Whether
    /// the object is active is a separate question.
    pub(crate) fn drawn_mesh(&self, object: ObjectHandle, renderer: &MeshRenderer) -> Option<u64> {
        let shown = renderer.kind == RendererKind::Mesh
            && renderer.switches.enabled
            && !renderer.switches.force_rendering_off;
        if shown {
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
                ComponentDesc::MeshFilter(filter) if filter.drawn_mesh != 0 => {
                    Some(filter.drawn_mesh)
                }
                _ => None,
            })
    }
}
