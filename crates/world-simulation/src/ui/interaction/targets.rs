use super::{TargetId, UiControlKind, UiRouter};
use crate::ui::{CanvasGroup, RectLayout, UiLayout};
use crate::{ObjectHandle, World, WorldError};
use glam::{DVec2, Vec2};
use std::collections::BTreeMap;

pub(super) struct Target<'a> {
    pub id: TargetId,
    pub rect: &'a RectLayout,
    pub regions: Option<Vec<[Vec2; 2]>>,
    pub clip: Option<[u32; 4]>,
    pub eligible: bool,
    pub blocking: bool,
}
pub(super) struct Targets<'a> {
    pub entries: Vec<Target<'a>>,
    pub modal: Option<ObjectHandle>,
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
                sorted.push((rect.draw_order(), world.ancestry(rect.object)?, rect));
            }
        }
        sorted.sort_by_key(|entry| entry.0);
        let modal = sorted
            .iter()
            .rev()
            .find(|(_, _, r)| router.modals.contains(&r.object))
            .map(|(_, _, r)| r.object);
        let mut result = Self {
            entries: Vec::new(),
            modal,
            height: f64::from(layout.viewport.y),
        };
        for (_, path, rect) in sorted {
            if modal.is_some_and(|m| !path.contains(&m)) {
                continue;
            }
            let Some(control) = router.controls.get(&rect.object) else {
                continue;
            };
            let (interactable, blocking) = flags(&path, groups);
            let target = Target {
                id: rect.object.into(),
                rect,
                regions: None,
                clip: None,
                eligible: control.enabled && interactable,
                blocking,
            };
            if control.kind == UiControlKind::Text {
                result
                    .entries
                    .extend(super::links::targets(world, layout, router, &target)?);
            } else {
                result.entries.push(target);
            }
        }
        Ok(result)
    }
    pub fn order(&self) -> Vec<TargetId> {
        self.entries
            .iter()
            .filter(|t| t.eligible)
            .map(|t| t.id)
            .collect()
    }
    pub fn interactable(&self, id: TargetId) -> bool {
        self.entries.iter().any(|t| t.id == id && t.eligible)
    }
    pub fn hit(&self, position: [f64; 2]) -> Option<TargetId> {
        let point = DVec2::new(position[0], self.height - position[1]).as_vec2();
        self.entries
            .iter()
            .rev()
            .find(|t| t.blocking && t.contains(point))
            .map(|t| t.id)
    }
}
impl Target<'_> {
    fn contains(&self, point: Vec2) -> bool {
        if !contains(self.rect, point) {
            return false;
        }
        if let Some([x, y, width, height]) = self.clip {
            let low = glam::UVec2::new(x, y).as_vec2();
            let high = glam::UVec2::new(x + width, y + height).as_vec2();
            if point.cmplt(low).any() || point.cmpge(high).any() {
                return false;
            }
        }
        let Some(regions) = &self.regions else {
            return true;
        };
        let Some(local) = coordinates(self.rect, point.as_dvec2()) else {
            return false;
        };
        let local = local.as_vec2() * self.rect.size;
        regions
            .iter()
            .any(|[low, high]| local.cmpge(*low).all() && local.cmplt(*high).all())
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
/// Scale the host delta before cross products so extreme finite external coordinates cannot
/// overflow into NaN. Shared by ordinary hits, linked glyphs and captured drags.
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
