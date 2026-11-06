use accesskit::{Action, Live, Node, NodeId, Rect, Role, Toggled};
use worldrunner_world_simulation::ui::{UiAccessAction, UiRole, UiSemanticNode, UiSemanticValue};

pub(super) fn node(source: &UiSemanticNode, children: Vec<NodeId>, live: bool) -> Node {
    let mut node = Node::new(role(source.role));
    node.set_children(children);
    let [x0, y0, x1, y1] = source.bounds;
    node.set_bounds(Rect::new(x0, y0, x1, y1));
    if source.role == UiRole::Status {
        node.set_live(if live { Live::Polite } else { Live::Off });
        node.set_value(source.label.clone());
    } else {
        node.set_label(source.label.clone());
    }
    if source.disabled {
        node.set_disabled();
    }
    for action in &source.actions {
        node.add_action(platform_action(*action));
    }
    value(&mut node, source.value);
    node
}
fn role(role: UiRole) -> Role {
    match role {
        UiRole::Window => Role::Window,
        UiRole::Group => Role::Group,
        UiRole::Button => Role::Button,
        UiRole::Checkbox => Role::CheckBox,
        UiRole::Slider => Role::Slider,
        UiRole::List => Role::List,
        UiRole::ListItem => Role::ListItem,
        UiRole::Dialog => Role::Dialog,
        UiRole::Text => Role::Label,
        UiRole::Status => Role::Status,
    }
}
fn value(node: &mut Node, value: UiSemanticValue) {
    match value {
        UiSemanticValue::None => {}
        UiSemanticValue::Toggled(on) => {
            node.set_toggled(if on { Toggled::True } else { Toggled::False });
        }
        UiSemanticValue::Selected(selected) => node.set_selected(selected),
        UiSemanticValue::Slider {
            value,
            min,
            max,
            step,
        } => {
            node.set_numeric_value(value);
            node.set_min_numeric_value(min);
            node.set_max_numeric_value(max);
            node.set_numeric_value_step(step.unwrap_or((max - min) / 100.));
        }
    }
}
fn platform_action(action: UiAccessAction) -> Action {
    match action {
        UiAccessAction::Focus => Action::Focus,
        UiAccessAction::Press | UiAccessAction::Toggle | UiAccessAction::Select => Action::Click,
        UiAccessAction::Increment => Action::Increment,
        UiAccessAction::Decrement => Action::Decrement,
        UiAccessAction::SetValue(_) => Action::SetValue,
    }
}
