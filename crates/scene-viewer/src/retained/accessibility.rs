//! Retained CPU semantic state and its optional native platform adapter.
use super::{RetainedEventHost, RetainedHost};
use accesskit::{Action, ActionData, ActionRequest, NodeId, Tree, TreeId, TreeUpdate};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use worldrunner_world_simulation::ui::{
    UiAccessAction, UiAccessRequest, UiNodeId, UiSemanticTree, UiSemanticUpdate,
};
mod nodes;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct Accessibility {
    tree: Option<UiSemanticTree>,
    ids: BTreeMap<UiNodeId, NodeId>,
    next: u64,
    live: BTreeSet<UiNodeId>,
    initial: Arc<Mutex<Option<TreeUpdate>>>,
    adapter: Option<accesskit_winit::Adapter>,
}
struct Initial(Arc<Mutex<Option<TreeUpdate>>>);
impl accesskit::ActivationHandler for Initial {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.lock().expect("accessibility snapshot").clone()
    }
}
impl Accessibility {
    fn id(&mut self, id: UiNodeId) -> NodeId {
        if id == UiNodeId::Window {
            return NodeId(0);
        }
        *self.ids.entry(id).or_insert_with(|| {
            self.next = self
                .next
                .checked_add(1)
                .expect("accessibility identity exhausted");
            NodeId(self.next)
        })
    }
    pub(crate) fn attach(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window: &winit::window::Window,
        proxy: winit::event_loop::EventLoopProxy<crate::app::UserEvent>,
    ) {
        self.adapter = Some(accesskit_winit::Adapter::with_mixed_handlers(
            event_loop,
            window,
            Initial(self.initial.clone()),
            proxy,
        ));
    }
    pub(crate) fn process_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) {
        if let Some(adapter) = &mut self.adapter {
            adapter.process_event(window, event);
        }
    }
    fn set(&mut self, update: UiSemanticUpdate) {
        if update.is_empty() {
            return;
        }
        for id in &update.removed {
            self.live.remove(id);
        }
        self.live
            .extend(update.announcements.iter().map(|a| a.node));
        let partial = self.translate(update.changed.iter(), update.focus);
        let tree = self.tree.get_or_insert_with(|| UiSemanticTree {
            nodes: BTreeMap::new(),
            focus: UiNodeId::Window,
        });
        for id in &update.removed {
            tree.nodes.remove(id);
        }
        for node in update.changed {
            tree.nodes.insert(node.id, node);
        }
        tree.focus = update.focus;
        let tree = tree.clone();
        let full = self.translate(tree.nodes.values(), tree.focus);
        *self.initial.lock().expect("accessibility snapshot") = Some(full);
        if let Some(adapter) = &mut self.adapter {
            adapter.update_if_active(|| partial);
        }
    }
    fn translate<'a>(
        &mut self,
        nodes: impl Iterator<Item = &'a worldrunner_world_simulation::ui::UiSemanticNode>,
        focus: UiNodeId,
    ) -> TreeUpdate {
        let nodes = nodes
            .map(|node| {
                let children: Vec<_> = node.children.iter().map(|id| self.id(*id)).collect();
                (
                    self.id(node.id),
                    nodes::node(node, children, self.live.contains(&node.id)),
                )
            })
            .collect();
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus: self.id(focus),
        }
    }
    pub(crate) fn request(&self, request: ActionRequest) -> Option<UiAccessRequest> {
        if request.target_tree != TreeId::ROOT {
            return None;
        }
        let target = if request.target_node == NodeId(0) {
            UiNodeId::Window
        } else {
            *self
                .ids
                .iter()
                .find(|(_, id)| **id == request.target_node)?
                .0
        };
        if !self.tree.as_ref()?.nodes.contains_key(&target) {
            return None;
        }
        let action = match request.action {
            Action::Click => UiAccessAction::Press,
            Action::Focus => UiAccessAction::Focus,
            Action::Increment => UiAccessAction::Increment,
            Action::Decrement => UiAccessAction::Decrement,
            Action::SetValue => UiAccessAction::SetValue(numeric_value(request.data?)?),
            _ => return None,
        };
        Some(UiAccessRequest { target, action })
    }
}
fn numeric_value(data: ActionData) -> Option<f64> {
    let value = match data {
        ActionData::NumericValue(value) => value,
        ActionData::Value(value) => value.parse().ok()?,
        _ => return None,
    };
    value.is_finite().then_some(value)
}
impl RetainedHost {
    /// Publish each router update once, after resolving the layout used for rendering/input.
    /// This service does not allocate a GPU, window, or adapter in windowless use.
    pub fn set_accessibility(&mut self, update: UiSemanticUpdate) {
        self.accessibility.set(update);
    }
    pub fn accessibility_tree(&self) -> Option<&UiSemanticTree> {
        self.accessibility.tree.as_ref()
    }
}
impl RetainedEventHost<'_> {
    pub fn set_accessibility(&mut self, update: UiSemanticUpdate) {
        self.host.set_accessibility(update);
    }
    pub fn accessibility_tree(&self) -> Option<&UiSemanticTree> {
        self.host.accessibility_tree()
    }
}
