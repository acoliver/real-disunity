//! The settings of a canvas group: one alpha and three interaction flags applied to every
//! UI element under the object that holds the group.
use crate::{ComponentDesc, ObjectHandle, UiError, World, WorldError};
use std::collections::BTreeMap;

/// The three interaction flags of a group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanvasGroupFlags {
    /// Whether the group's elements take input. The engine sends no UI pointer events yet.
    pub interactable: bool,
    /// Whether the group's elements count as hit by a pointer inside their rectangle.
    pub blocks_raycasts: bool,
    /// Whether the group ignores the groups above it, so their alpha and flags stop here.
    pub ignore_parent_groups: bool,
}
impl Default for CanvasGroupFlags {
    fn default() -> Self {
        Self {
            interactable: true,
            blocks_raycasts: true,
            ignore_parent_groups: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasGroup {
    pub enabled: bool,
    /// Opacity from 0 (invisible) to 1 (opaque).
    pub alpha: f32,
    pub flags: CanvasGroupFlags,
}
impl Default for CanvasGroup {
    fn default() -> Self {
        Self {
            enabled: true,
            alpha: 1.,
            flags: CanvasGroupFlags::default(),
        }
    }
}
impl CanvasGroup {
    /// Clamps the alpha into 0 through 1. A NaN alpha is rejected.
    pub fn normalized(mut self) -> Result<Self, UiError> {
        if self.alpha.is_nan() {
            return Err(UiError::InvalidCanvasGroup);
        }
        self.alpha = self.alpha.clamp(0., 1.);
        Ok(self)
    }

    pub(crate) fn validate(self) -> Result<(), UiError> {
        if (0. ..=1.).contains(&self.alpha) {
            Ok(())
        } else {
            Err(UiError::InvalidCanvasGroup)
        }
    }
}

/// The enabled groups of a world by owning object.
pub(crate) fn groups_by_object(
    components: &[crate::ComponentSnapshot],
) -> BTreeMap<ObjectHandle, CanvasGroup> {
    components
        .iter()
        .filter_map(|c| match c.desc {
            ComponentDesc::CanvasGroup(group) if group.enabled => Some((c.object, group)),
            _ => None,
        })
        .collect()
}

/// The alpha an element is drawn with: the product of the alpha of every enabled group on the
/// element and its ancestors, ending at the first group that ignores the groups above it.
pub(crate) fn effective_alpha(
    world: &World,
    groups: &BTreeMap<ObjectHandle, CanvasGroup>,
    object: ObjectHandle,
) -> Result<f32, WorldError> {
    let mut alpha = 1.;
    let mut current = Some(object);
    while let Some(node) = current {
        if let Some(group) = groups.get(&node) {
            alpha *= group.alpha;
            if group.flags.ignore_parent_groups {
                break;
            }
        }
        current = world.parent(node)?;
    }
    Ok(alpha)
}
