/// The three flags of a canvas group of a player scene, as the managed `CanvasGroup` takes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CanvasGroupInput {
    pub interactable: bool,
    pub blocks_raycasts: bool,
    /// Whether the group ignores the groups above it.
    pub ignore_parent_groups: bool,
}
