//! Renderer lists: what a script asks the host to draw from a culling result.
//!
//! A script turns a culling result into a list (the renderers one render pass draws, or the
//! shadow casters of one light) and later draws the list through a command. The world records
//! both as offscreen work; the host resolves a list when it reaches the draw record. A list
//! belongs to the frame that created it: the frame's end forgets it, like a culling result.
use super::properties::check_name;
use super::{GraphicsError, MAX_CULL_PLANES, MaterialId, OffscreenWork};
use crate::{World, WorldError};

/// Most pass tags (`LightMode` names) one drawing list holds.
pub const MAX_LIST_PASS_TAGS: usize = 16;

/// The first id the world issues for a list. Culling ids come from the script's own counter and
/// stay far below it, so a list never takes an id a later culling result needs.
const FIRST_LIST_ID: u64 = 1 << 63;

/// What a renderer list draws.
#[derive(Clone, Debug, PartialEq)]
pub enum RendererListDesc {
    /// The renderers a culling result kept whose material has a pass with one of `pass_tags`
    /// (`LightMode` values, 1 to 16 names), filtered and sorted as given.
    Draw {
        /// Id of a culling record of this frame.
        cull: u64,
        pass_tags: Vec<String>,
        /// Inclusive render queue range, `queue_min <= queue_max`.
        queue_min: i32,
        queue_max: i32,
        layer_mask: u32,
        rendering_layer_mask: u32,
        /// Sorting criteria bits.
        sorting: u32,
        /// Per-object data bits.
        per_object: u32,
        override_material: Option<MaterialId>,
        override_pass: i32,
    },
    /// The shadow casters a culling result kept for one visible light.
    Shadow {
        /// Id of a culling record of this frame.
        cull: u64,
        /// Index of the light among the culling result's visible lights, at least 0.
        light_index: i32,
        /// Split culling planes, 0 to 10, each a normal and a distance.
        planes: Vec<[f32; 4]>,
        /// Split culling sphere: centre and radius.
        sphere: [f32; 4],
        /// 0 for all objects, 1 for dynamic only, 2 for static only.
        objects_filter: u32,
        /// 0 for unknown, 1 for perspective, 2 for orthographic.
        projection: u32,
    },
}

impl RendererListDesc {
    /// The culling result the list reads.
    const fn cull(&self) -> u64 {
        match self {
            Self::Draw { cull, .. } | Self::Shadow { cull, .. } => *cull,
        }
    }
}

impl World {
    /// Records a renderer list at this place in the frame's offscreen list and returns its id,
    /// which a [`OffscreenWork::DrawRendererList`] names. The list must read a culling result
    /// recorded earlier in the frame. A drawing list holds 1 to 16 non-empty pass tags of at
    /// most [`MAX_NAME_BYTES`](super::MAX_NAME_BYTES) bytes, a queue range with its minimum at
    /// most its maximum, and a live override material when it has one. A shadow list names a
    /// light index of at least 0, at most 10 planes, an objects filter of 0 to 2 and a projection
    /// of 0 to 2. Every float must be finite. A refused list records nothing. The id is nonzero
    /// and shares the frame's handle set with the culling results, so it never collides with
    /// one.
    pub fn create_renderer_list(&mut self, desc: RendererListDesc) -> Result<u64, WorldError> {
        let id = self.next_list_id()?;
        self.push_offscreen(OffscreenWork::CreateRendererList { id, desc })?;
        Ok(id)
    }

    /// Checks the renderer list records; other work is the offscreen list's own to check.
    pub(super) fn check_renderer_list(&self, work: &OffscreenWork) -> Result<(), WorldError> {
        match work {
            OffscreenWork::CreateRendererList { desc, .. } => {
                if !self.graphics.handles.contains(&desc.cull())
                    || self.graphics.lists.contains(&desc.cull())
                {
                    return Err(GraphicsError::InvalidOffscreen.into());
                }
                match desc {
                    RendererListDesc::Draw { .. } => self.check_draw_list(desc),
                    RendererListDesc::Shadow { .. } => check_shadow_list(desc),
                }
            }
            OffscreenWork::DrawRendererList { list } => {
                if self.graphics.lists.contains(list) {
                    Ok(())
                } else {
                    Err(GraphicsError::InvalidOffscreen.into())
                }
            }
            _ => Ok(()),
        }
    }

    fn check_draw_list(&self, desc: &RendererListDesc) -> Result<(), WorldError> {
        let RendererListDesc::Draw {
            pass_tags,
            queue_min,
            queue_max,
            override_material,
            ..
        } = desc
        else {
            return Ok(());
        };
        if pass_tags.is_empty() || pass_tags.len() > MAX_LIST_PASS_TAGS || queue_min > queue_max {
            return Err(GraphicsError::InvalidOffscreen.into());
        }
        pass_tags.iter().try_for_each(|tag| check_name(tag))?;
        if let Some(material) = override_material {
            self.material(*material)?;
        }
        Ok(())
    }

    /// The id the next list takes: above every list of the frame and unused by any handle.
    fn next_list_id(&self) -> Result<u64, WorldError> {
        let mut id = self
            .graphics
            .lists
            .last()
            .map_or(Some(FIRST_LIST_ID), |last| last.checked_add(1));
        while let Some(candidate) = id {
            if !self.graphics.handles.contains(&candidate) {
                return Ok(candidate);
            }
            id = candidate.checked_add(1);
        }
        Err(WorldError::CapacityExceeded(crate::LimitKind::Graphics))
    }
}

fn check_shadow_list(desc: &RendererListDesc) -> Result<(), WorldError> {
    let RendererListDesc::Shadow {
        light_index,
        planes,
        sphere,
        objects_filter,
        projection,
        ..
    } = desc
    else {
        return Ok(());
    };
    if *light_index < 0 || planes.len() > MAX_CULL_PLANES || *objects_filter > 2 || *projection > 2
    {
        return Err(GraphicsError::InvalidOffscreen.into());
    }
    if planes
        .iter()
        .flatten()
        .chain(sphere)
        .any(|v| !v.is_finite())
    {
        return Err(GraphicsError::NotFinite.into());
    }
    Ok(())
}
