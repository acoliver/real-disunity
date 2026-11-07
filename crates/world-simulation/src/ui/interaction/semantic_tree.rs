use super::{
    Targets, UiControlKind, UiRouter, groups_by_object,
    semantics::{
        UiAccessAction, UiAnnouncement, UiNodeId, UiRole, UiSemanticNode, UiSemanticTree,
        UiSemanticUpdate, UiSemanticValue,
    },
};
use crate::ui::{RectLayout, UiLayout};
use crate::{ComponentDesc, ObjectHandle, World, WorldError};
use std::collections::{BTreeMap, BTreeSet};

fn attach_rects(
    world: &World,
    targets: &Targets<'_>,
    nodes: &mut BTreeMap<UiNodeId, UiSemanticNode>,
) -> Result<(), WorldError> {
    for rect in &targets.rects {
        let parent = world
            .ancestry(rect.object)?
            .into_iter()
            .rev()
            .skip(1)
            .map(UiNodeId::Object)
            .find(|id| nodes.contains_key(id))
            .unwrap_or(UiNodeId::Window);
        nodes
            .get_mut(&parent)
            .expect("tree parent")
            .children
            .push(UiNodeId::Object(rect.object));
    }
    Ok(())
}

impl UiRouter {
    /// Resolve against the same fresh physical-pixel layout passed to input and rendering.
    /// This also revalidates focus and modal history when no input event has occurred.
    pub fn semantic_tree(
        &mut self,
        world: &World,
        layout: &UiLayout,
    ) -> Result<UiSemanticTree, WorldError> {
        let groups = groups_by_object(world.published_components());
        let targets = Targets::new(world, layout, self, &groups)?;
        self.modal_changed(targets.modal);
        self.retain_focus(|object| targets.interactable(object));
        let texts: BTreeMap<_, _> = world
            .published_components()
            .filter_map(|c| match &c.desc {
                ComponentDesc::Text(run) if run.enabled => Some((c.object, run.text.clone())),
                _ => None,
            })
            .collect();
        let lists = self
            .controls
            .values()
            .filter_map(|c| match c.kind {
                UiControlKind::ListItem { list, .. }
                | UiControlKind::Choice { group: list, .. } => Some(list),
                _ => None,
            })
            .collect();
        let mut nodes = BTreeMap::new();
        nodes.insert(UiNodeId::Window, window(layout));
        for rect in &targets.rects {
            let node = self.semantic_node(rect, &targets, &texts, &lists, layout);
            nodes.insert(node.id, node);
        }
        attach_rects(world, &targets, &mut nodes)?;
        self.control_labels(world, &texts, &mut nodes)?;
        super::semantic_links::append(world, layout, &targets, &mut nodes);
        Ok(UiSemanticTree {
            nodes,
            focus: self.focus.map_or(UiNodeId::Window, UiNodeId::from),
        })
    }
    pub fn semantic_update(
        &mut self,
        world: &World,
        layout: &UiLayout,
    ) -> Result<UiSemanticUpdate, WorldError> {
        let tree = self.semantic_tree(world, layout)?;
        let mut update = tree.changes(self.semantic_previous.as_ref());
        self.semantic_live
            .retain(|object, _| world.object_alive(*object));
        for node in tree
            .nodes
            .values()
            .filter(|node| node.role == UiRole::Status)
        {
            let UiNodeId::Object(object) = node.id else {
                unreachable!("object status");
            };
            if self.semantic_live.get(&object) != Some(&node.label) {
                self.semantic_live.insert(object, node.label.clone());
                if !node.label.is_empty() {
                    update.announcements.push(UiAnnouncement {
                        node: node.id,
                        text: node.label.clone(),
                    });
                }
            }
        }
        self.semantic_previous = Some(tree);
        Ok(update)
    }
    fn semantic_node(
        &self,
        rect: &RectLayout,
        targets: &Targets<'_>,
        texts: &BTreeMap<ObjectHandle, String>,
        lists: &BTreeSet<ObjectHandle>,
        layout: &UiLayout,
    ) -> UiSemanticNode {
        let object = rect.object;
        let metadata = self.semantics.get(&object);
        let role = if targets.modal == Some(object) {
            UiRole::Dialog
        } else if metadata.is_some_and(|s| s.live) {
            UiRole::Status
        } else {
            self.semantic_role(object, texts, lists)
        };
        let value = self.semantic_value(object);
        let eligible = targets.interactable(object.into());
        UiSemanticNode {
            id: UiNodeId::Object(object),
            role,
            value,
            label: metadata
                .and_then(|s| s.label.clone())
                .or_else(|| texts.get(&object).cloned())
                .unwrap_or_default(),
            bounds: bounds(rect, layout),
            children: Vec::new(),
            actions: actions(value, eligible, role),
            disabled: self.controls.contains_key(&object) && !eligible,
        }
    }
    fn semantic_role(
        &self,
        object: ObjectHandle,
        texts: &BTreeMap<ObjectHandle, String>,
        lists: &BTreeSet<ObjectHandle>,
    ) -> UiRole {
        if let Some(control) = self.controls.get(&object) {
            return match control.kind {
                UiControlKind::Button => UiRole::Button,
                UiControlKind::Text | UiControlKind::TextField => UiRole::Text,
                UiControlKind::Toggle { .. } => UiRole::Checkbox,
                UiControlKind::Slider(_) => UiRole::Slider,
                UiControlKind::ListItem { .. } | UiControlKind::Choice { .. } => UiRole::ListItem,
            };
        }
        if lists.contains(&object) {
            UiRole::List
        } else if texts.contains_key(&object) {
            UiRole::Text
        } else {
            UiRole::Group
        }
    }
    fn semantic_value(&self, object: ObjectHandle) -> UiSemanticValue {
        match self.controls.get(&object).map(|c| c.kind) {
            Some(UiControlKind::Toggle { on }) => UiSemanticValue::Toggled(on),
            Some(UiControlKind::Slider(slider)) => UiSemanticValue::Slider {
                value: slider.value(),
                min: slider.min(),
                max: slider.max(),
                step: slider.step(),
            },
            Some(UiControlKind::Choice { group, index }) => {
                UiSemanticValue::Selected(self.selected(group) == Some(index))
            }
            Some(UiControlKind::ListItem { list, index }) => {
                UiSemanticValue::Selected(self.selected(list) == Some(index))
            }
            _ => UiSemanticValue::None,
        }
    }
    fn control_labels(
        &self,
        world: &World,
        texts: &BTreeMap<ObjectHandle, String>,
        nodes: &mut BTreeMap<UiNodeId, UiSemanticNode>,
    ) -> Result<(), WorldError> {
        for (&object, text) in texts {
            if !nodes.contains_key(&UiNodeId::Object(object)) {
                continue;
            }
            let control = world
                .ancestry(object)?
                .into_iter()
                .rev()
                .skip(1)
                .find(|o| self.controls.contains_key(o));
            if let Some(control) = control
                && self
                    .semantics
                    .get(&control)
                    .is_none_or(|s| s.label.is_none())
                && let Some(node) = nodes.get_mut(&UiNodeId::Object(control))
            {
                if !node.label.is_empty() {
                    node.label.push(' ');
                }
                node.label.push_str(text);
            }
        }
        Ok(())
    }
}
fn actions(value: UiSemanticValue, eligible: bool, role: UiRole) -> Vec<UiAccessAction> {
    use UiAccessAction::{Decrement, Focus, Increment, Press, Select, SetValue, Toggle};
    if !eligible {
        return Vec::new();
    }
    if role == UiRole::Text {
        return vec![Focus];
    }
    match value {
        UiSemanticValue::Slider { value, .. } => vec![Focus, Increment, Decrement, SetValue(value)],
        UiSemanticValue::Toggled(_) => vec![Focus, Press, Toggle],
        UiSemanticValue::Selected(_) => vec![Focus, Press, Select],
        UiSemanticValue::None => vec![Focus, Press],
    }
}
fn bounds(rect: &RectLayout, layout: &UiLayout) -> [f64; 4] {
    let corners = rect.screen_corners.expect("screen target");
    let min = corners
        .into_iter()
        .fold(glam::Vec2::splat(f32::INFINITY), glam::Vec2::min);
    let max = corners
        .into_iter()
        .fold(glam::Vec2::splat(f32::NEG_INFINITY), glam::Vec2::max);
    [
        f64::from(min.x),
        f64::from(layout.viewport.y) - f64::from(max.y),
        f64::from(max.x),
        f64::from(layout.viewport.y) - f64::from(min.y),
    ]
}
fn window(layout: &UiLayout) -> UiSemanticNode {
    UiSemanticNode {
        id: UiNodeId::Window,
        role: UiRole::Window,
        label: String::new(),
        value: UiSemanticValue::None,
        bounds: [
            0.,
            0.,
            f64::from(layout.viewport.x),
            f64::from(layout.viewport.y),
        ],
        children: Vec::new(),
        actions: Vec::new(),
        disabled: false,
    }
}
