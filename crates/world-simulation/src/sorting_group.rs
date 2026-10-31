//! The sorting group of an object: its renderers and its descendants' renderers sort together
//! as one unit under the group's layer and order. Nothing draws with it yet; the values are kept
//! so the render path can use them.

/// The settings of a sorting group. Sorting layer 0 is the only layer the engine registers.
/// The order is stored as given, like a renderer's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortingGroup {
    pub enabled: bool,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    /// Sorts the group against the other groups at the root of the hierarchy instead of its
    /// parent group.
    pub sort_at_root: bool,
}

impl Default for SortingGroup {
    fn default() -> Self {
        Self {
            enabled: true,
            sorting_layer: 0,
            sorting_order: 0,
            sort_at_root: false,
        }
    }
}
