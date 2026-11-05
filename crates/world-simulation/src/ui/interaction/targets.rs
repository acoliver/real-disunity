use super::UiRouter;
use crate::ui::{CanvasGroup, RectLayout, UiLayout};
use crate::{ObjectHandle, World, WorldError};
use glam::{DVec2, Vec2};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Targets<'a> {
    pub rects: Vec<&'a RectLayout>,
    pub modal: Option<ObjectHandle>,
    eligible: BTreeSet<ObjectHandle>,
    blocking: BTreeSet<ObjectHandle>,
    height: f64,
}
impl<'a> Targets<'a> {
    pub fn new(
        world: &World,
        layout: &'a UiLayout,
        router: &UiRouter,
        groups: &BTreeMap<ObjectHandle, CanvasGroup>,
    ) -> Result<Self, WorldError> {
        let mut sorted = Vec::new();
        for rect in &layout.rects {
            if rect.on_screen()
                && world.object_alive(rect.object)
                && world.active_in_hierarchy(rect.object)?
            {
                let path = world.ancestry(rect.object)?;
                sorted.push((rect.draw_order(), path, rect));
            }
        }
        sorted.sort_by_key(|entry| entry.0);
        let modal = sorted
            .iter()
            .rev()
            .find(|(_, _, r)| router.modals.contains(&r.object))
            .map(|(_, _, r)| r.object);
        let mut result = Self {
            rects: Vec::new(),
            modal,
            eligible: BTreeSet::new(),
            blocking: BTreeSet::new(),
            height: f64::from(layout.viewport.y),
        };
        for (_, path, rect) in sorted {
            if modal.is_some_and(|m| !path.contains(&m)) {
                continue;
            }
            let (interactable, blocks) = flags(&path, groups);
            if router.controls.get(&rect.object).is_some_and(|c| c.enabled) && interactable {
                result.eligible.insert(rect.object);
            }
            if blocks && router.controls.contains_key(&rect.object) {
                result.blocking.insert(rect.object);
            }
            result.rects.push(rect);
        }
        Ok(result)
    }
    pub fn interactable(&self, object: ObjectHandle) -> bool {
        self.eligible.contains(&object)
    }
    pub fn hit(&self, position: [f64; 2]) -> Option<ObjectHandle> {
        let point = DVec2::new(position[0], self.height - position[1]).as_vec2();
        self.rects
            .iter()
            .rev()
            .find(|r| self.blocking.contains(&r.object) && contains(r, point))
            .map(|r| r.object)
    }
}
fn flags(path: &[ObjectHandle], groups: &BTreeMap<ObjectHandle, CanvasGroup>) -> (bool, bool) {
    let (mut interactable, mut blocks) = (true, true);
    for group in path.iter().rev().filter_map(|o| groups.get(o)) {
        interactable &= group.flags.interactable;
        blocks &= group.flags.blocks_raycasts;
        if group.flags.ignore_parent_groups {
            break;
        }
    }
    (interactable, blocks)
}
/// Invert the screen-space affine basis, preserving rotated and sheared hit regions.
pub(crate) fn contains(rect: &RectLayout, point: Vec2) -> bool {
    if rect.clip_bounds.is_some_and(|[min, max]| {
        point.x < min.x || point.y < min.y || point.x >= max.x || point.y >= max.y
    }) {
        return false;
    }
    let Some(local) = coordinates(rect, point.as_dvec2()) else {
        return false;
    };
    (0. ..=1.).contains(&local.x) && (0. ..=1.).contains(&local.y)
}
/// Shared affine coordinates for hits and captured drags. Scale the host delta before cross
/// products so even extreme finite external coordinates cannot overflow into NaN.
pub(super) fn coordinates(rect: &RectLayout, point: DVec2) -> Option<DVec2> {
    let [origin, right, _, up] = rect.screen_corners?;
    let x = right.as_dvec2() - origin.as_dvec2();
    let y = up.as_dvec2() - origin.as_dvec2();
    let determinant = x.perp_dot(y);
    if determinant == 0. {
        return None;
    }
    let delta = point - origin.as_dvec2();
    let scale = delta.abs().max_element().max(1.);
    let delta = delta / scale;
    Some(DVec2::new(delta.perp_dot(y), x.perp_dot(delta)) / determinant * scale)
}
