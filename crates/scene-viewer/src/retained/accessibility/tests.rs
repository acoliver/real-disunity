use super::*;
use crate::{RetainedDriver, RetainedEvent, RetainedTick, app::Content, retained::Session};
use worldrunner_world_simulation::{World, WorldSettings};

#[test]
fn windowless_host_exposes_tree_without_an_adapter() {
    let w = World::new(WorldSettings::default()).unwrap();
    let mut router = worldrunner_world_simulation::ui::UiRouter::default();
    let layout = w.ui_layout([320, 200].into()).unwrap();
    let update = router.semantic_update(&w, &layout).unwrap();
    let full = router.semantic_tree(&w, &layout).unwrap();
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    host.set_accessibility(update);
    assert_eq!(host.accessibility_tree(), Some(&full));
    assert!(host.accessibility.adapter.is_none());
}

#[test]
fn accesskit_action_is_delivered_to_the_driver_without_a_frame() {
    struct Driver(Vec<UiAccessRequest>);
    impl RetainedDriver for Driver {
        fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
            panic!("no frame should run");
        }
        fn event(
            &mut self,
            _: &mut RetainedEventHost<'_>,
            event: &RetainedEvent,
        ) -> Result<(), String> {
            if let RetainedEvent::Accessibility(request) = event {
                self.0.push(*request);
            }
            Ok(())
        }
    }
    let mut driver = Driver(Vec::new());
    let host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let mut session = Session::new(host, &mut driver);
    session.accessibility_action(UiAccessRequest {
        target: UiNodeId::Window,
        action: UiAccessAction::Focus,
    });
    session.shutdown().unwrap();
    drop(session);
    assert_eq!(driver.0.len(), 1);
}

fn semantic_fixture() -> (
    World,
    worldrunner_world_simulation::ui::UiRouter,
    worldrunner_world_simulation::ObjectHandle,
) {
    use worldrunner_world_simulation::ui::{
        Canvas, RectTransform, UiControl, UiRouter, UiSemantics,
    };
    use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, ParentSpace};
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    world
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let button = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            button,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .set_parent(button, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    let mut router = UiRouter::default();
    router.controls.insert(button, UiControl::button());
    router.semantics.insert(button, UiSemantics::label("Play"));
    (world, router, button)
}

#[test]
fn platform_requests_map_to_router_actions_with_stable_platform_ids() {
    use worldrunner_world_simulation::ui::{UiAction, UiKey};
    let (world, mut router, button) = semantic_fixture();
    let layout = world.ui_layout([400, 300].into()).unwrap();
    let mut service = Accessibility::default();
    service.set(router.semantic_update(&world, &layout).unwrap());
    let id = service.ids[&UiNodeId::Object(button)];
    let request = service
        .request(ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: id,
            data: None,
        })
        .unwrap();
    assert_eq!(
        router
            .accessibility_action(&world, &layout, request)
            .unwrap()
            .actions,
        [UiAction::Activated { object: button }]
    );
    router
        .key(&world, &layout, UiKey::Tab, true, false)
        .unwrap();
    service.set(router.semantic_update(&world, &layout).unwrap());
    let mut initial = Initial(service.initial.clone());
    let update = accesskit::ActivationHandler::request_initial_tree(&mut initial).unwrap();
    assert_eq!(service.ids[&UiNodeId::Object(button)], id);
    assert_eq!(update.focus, id);
    let node = &update.nodes.iter().find(|(node, _)| *node == id).unwrap().1;
    assert_eq!(node.role(), accesskit::Role::Button);
    assert_eq!(node.label(), Some("Play"));
    assert!(node.supports_action(Action::Click));
    assert!(
        service
            .request(ActionRequest {
                action: Action::SetValue,
                target_tree: TreeId::ROOT,
                target_node: id,
                data: Some(ActionData::NumericValue(f64::NAN)),
            })
            .is_none()
    );
}

#[test]
fn platform_live_text_has_a_value_and_does_not_announce_modal_restoration() {
    use worldrunner_world_simulation::ui::UiSemantics;
    let (world, mut router, button) = semantic_fixture();
    router.controls.remove(&button);
    router.semantics.insert(
        button,
        UiSemantics {
            label: Some("Ready".into()),
            live: true,
        },
    );
    let layout = world.ui_layout([400, 300].into()).unwrap();
    let mut service = Accessibility::default();
    service.set(router.semantic_update(&world, &layout).unwrap());
    let id = service.ids[&UiNodeId::Object(button)];
    let snapshot = service.initial.lock().unwrap().clone().unwrap();
    let node = &snapshot
        .nodes
        .iter()
        .find(|(node, _)| *node == id)
        .unwrap()
        .1;
    assert_eq!(node.value(), Some("Ready"));
    assert_eq!(node.live(), Some(accesskit::Live::Polite));
    let mut hidden = layout.clone();
    hidden.rects.retain(|rect| rect.object != button);
    service.set(router.semantic_update(&world, &hidden).unwrap());
    assert!(
        service
            .request(ActionRequest {
                action: Action::Focus,
                target_tree: TreeId::ROOT,
                target_node: id,
                data: None,
            })
            .is_none()
    );
    service.set(router.semantic_update(&world, &layout).unwrap());
    let snapshot = service.initial.lock().unwrap().clone().unwrap();
    let node = &snapshot
        .nodes
        .iter()
        .find(|(node, _)| *node == id)
        .unwrap()
        .1;
    assert_eq!(node.live(), Some(accesskit::Live::Off));
}

fn platform_link() -> (Accessibility, UiNodeId) {
    use worldrunner_world_simulation::ui::{UiLinkId, UiRole};
    let (world, mut router, object) = semantic_fixture();
    let layout = world.ui_layout([400, 300].into()).unwrap();
    let mut update = router.semantic_update(&world, &layout).unwrap();
    let parent = update
        .changed
        .iter_mut()
        .find(|n| n.id == UiNodeId::Object(object))
        .unwrap();
    parent.role = UiRole::Text;
    parent.actions.clear();
    let mut source = parent.clone();
    source.id = UiNodeId::Link {
        object,
        link: UiLinkId(7),
    };
    source.role = UiRole::Link;
    source.label = "Read more".into();
    source.actions = vec![UiAccessAction::Focus, UiAccessAction::Press];
    parent.children.push(source.id);
    update.focus = source.id;
    let target = source.id;
    update.changed.push(source);
    let mut service = Accessibility::default();
    service.set(update);
    (service, target)
}

#[test]
fn link_semantics_publish_role_click_focus_and_round_trip_span_identity() {
    let (service, target) = platform_link();
    let id = service.ids[&target];
    let full = service.initial.lock().unwrap().clone().unwrap();
    assert_eq!(full.focus, id);
    let node = &full.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
    assert_eq!(node.role(), accesskit::Role::Link);
    assert_eq!(node.label(), Some("Read more"));
    let [left, top, right, bottom] = service.tree.as_ref().unwrap().nodes[&target].bounds;
    assert_eq!(
        node.bounds(),
        Some(accesskit::Rect::new(left, top, right, bottom))
    );
    let parent_id = service
        .tree
        .as_ref()
        .unwrap()
        .nodes
        .values()
        .find(|n| n.children.contains(&target))
        .unwrap()
        .id;
    let parent = &full
        .nodes
        .iter()
        .find(|(id, _)| *id == service.ids[&parent_id])
        .unwrap()
        .1;
    assert_eq!(parent.role(), accesskit::Role::Label);
    assert_eq!(parent.children(), &[id]);
    assert!(node.supports_action(Action::Focus));
    assert!(node.supports_action(Action::Click));
    for (action, expected) in [
        (Action::Click, UiAccessAction::Press),
        (Action::Focus, UiAccessAction::Focus),
    ] {
        assert_eq!(
            service.request(ActionRequest {
                action,
                target_tree: TreeId::ROOT,
                target_node: id,
                data: None
            }),
            Some(UiAccessRequest {
                target,
                action: expected
            })
        );
    }
}

#[test]
fn removed_platform_link_rejects_obsolete_requests() {
    let (mut service, target) = platform_link();
    let id = service.ids[&target];
    let UiNodeId::Link { object, .. } = target else {
        unreachable!()
    };
    let mut parent = service.tree.as_ref().unwrap().nodes[&UiNodeId::Object(object)].clone();
    parent.children.clear();
    service.set(UiSemanticUpdate {
        changed: vec![parent],
        removed: vec![target],
        focus: UiNodeId::Window,
        focus_changed: true,
        announcements: vec![],
    });
    assert!(
        service
            .request(ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: id,
                data: None
            })
            .is_none()
    );
}
