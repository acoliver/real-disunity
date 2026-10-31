//! The `SortingGroup` layout, which sorts the renderers under one object as a unit.
//!
//! The field order comes from the engine's serialized form as written by the editor. Old World's
//! own files contain no sorting group, so no stored object has confirmed it; a player whose
//! group does not fit reports a decode error, which the planner turns into a default group.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortingGroup {
    pub game_object: PPtr,
    pub enabled: bool,
    pub sorting_layer_id: i32,
    pub sorting_layer: i16,
    pub sorting_order: i16,
    /// Whether the group sorts against the other groups at the root of the hierarchy.
    pub sort_at_root: bool,
}

impl Layout for SortingGroup {
    const CLASS: ClassId = ClassId::SORTING_GROUP;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        r.align()?;
        let sorting_layer_id = r.i32()?;
        let sorting_layer = r.i16()?;
        let sorting_order = r.i16()?;
        let sort_at_root = r.bool()?;
        r.align()?;
        Ok(Self {
            game_object,
            enabled,
            sorting_layer_id,
            sorting_layer,
            sorting_order,
            sort_at_root,
        })
    }
}
