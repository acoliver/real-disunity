use super::*;
use std::collections::BTreeMap;

#[test]
fn affine_scaled_link_bounds_match_the_extracted_snapped_glyphs() {
    let (mut w, mut r, fonts, root, text) = fixture();
    let canvas = w
        .published_components()
        .find(|c| c.object == root && matches!(c.desc, ComponentDesc::Canvas(_)))
        .unwrap()
        .handle;
    w.set_component(
        canvas,
        ComponentDesc::Canvas(Canvas {
            pixel_perfect: true,
            ..Default::default()
        }),
    )
    .unwrap();
    w.set_local_transform(
        text,
        worldrunner_world_simulation::LocalTransform {
            rotation: glam::Quat::from_rotation_z(0.3),
            scale: glam::Vec3::new(1.1, 0.8, 1.),
            ..Default::default()
        },
    )
    .unwrap();
    r.layout.display_scale = 1.5;
    let t = tree(&w, &mut r);
    let ids = children(&t, text);
    assert_eq!(ids.len(), 1);
    let p = plan(&w, &r, &fonts);
    assert_eq!(
        t.nodes[&ids[0]].bounds.map(f64::to_bits),
        glyph_bounds(&p, UiLinkId(7)).map(f64::to_bits)
    );
}

#[test]
fn assistive_focus_reveals_the_link_and_modal_restoration_keeps_its_identity() {
    let (mut w, mut r, _, root, text) = fixture();
    two_links(&mut w, text);
    let view = rect(&mut w, Some(root), Vec2::new(30., 20.));
    w.set_parent(text, Some(view), ParentSpace::KeepLocal)
        .unwrap();
    r.layout.scrolls.insert(view, UiScrollView::new(text));
    let ids = children(&tree(&w, &mut r), text);
    assert_eq!(ids.len(), 2);
    request(&w, &mut r, ids[1], UiAccessAction::Focus);
    assert!(r.layout.scrolls[&view].offset().y > 0.);
    assert_eq!(tree(&w, &mut r).focus, ids[1]);
    let modal = rect(&mut w, Some(root), Vec2::ONE);
    r.controls.insert(modal, UiControl::button());
    r.modals.insert(modal);
    let restricted = tree(&w, &mut r);
    assert!(!restricted.nodes.contains_key(&ids[1]));
    assert!(!request(&w, &mut r, ids[1], UiAccessAction::Press).consumed);
    key(&w, &mut r, UiKey::Tab);
    r.modals.clear();
    assert_eq!(tree(&w, &mut r).focus, ids[1]);
    assert_eq!(r.focused_link(), Some((text, UiLinkId(2))));
}

#[test]
fn inherited_noninteractable_paragraph_does_not_publish_links() {
    let (mut w, mut r, _, root, text) = fixture();
    w.add_component(
        root,
        ComponentDesc::CanvasGroup(CanvasGroup {
            flags: CanvasGroupFlags {
                interactable: false,
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(children(&tree(&w, &mut r), text).is_empty());
    assert_eq!(tree(&w, &mut r).nodes.len(), 3);
}

fn tree(w: &World, r: &mut UiRouter) -> UiSemanticTree {
    r.semantic_tree(w, &layout(w, r)).unwrap()
}
fn children(t: &UiSemanticTree, text: ObjectHandle) -> Vec<UiNodeId> {
    t.nodes[&UiNodeId::Object(text)].children.clone()
}
fn request(w: &World, r: &mut UiRouter, target: UiNodeId, action: UiAccessAction) -> UiResponse {
    r.accessibility_action(w, &layout(w, r), UiAccessRequest { target, action })
        .unwrap()
}
fn glyph_bounds(p: &contribution::UiRenderPlan, link: UiLinkId) -> [f64; 4] {
    p.texts[0]
        .quads
        .iter()
        .filter(|q| q.link == Some(link))
        .flat_map(|q| q.corners)
        .fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |b, c| {
                let (x, y) = (f64::from(c[0]), 100. - f64::from(c[1]));
                [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)]
            },
        )
}

#[test]
fn two_link_snapshot_has_named_glyph_bounds_and_reading_order_children() {
    let (mut w, mut r, fonts, root, text) = fixture();
    two_links(&mut w, text);
    edit_run(&mut w, text, |run| run.text = "AB CD".into());
    let t = tree(&w, &mut r);
    assert_eq!(t.nodes.len(), 5);
    assert_eq!(
        t.nodes[&UiNodeId::Window].children,
        [UiNodeId::Object(root)]
    );
    assert_eq!(
        t.nodes[&UiNodeId::Object(root)].children,
        [UiNodeId::Object(text)]
    );
    assert_eq!(t.nodes[&UiNodeId::Object(text)].role, UiRole::Text);
    assert_eq!(t.nodes[&UiNodeId::Object(text)].label, "AB CD");
    assert!(!t.nodes[&UiNodeId::Object(text)].disabled);
    assert!(t.nodes[&UiNodeId::Object(text)].actions.is_empty());
    let ids = children(&t, text);
    assert_eq!(ids.len(), 2);
    let p = plan(&w, &r, &fonts);
    for ((id, name), link) in ids.iter().zip(["AB", "CD"]).zip([UiLinkId(9), UiLinkId(2)]) {
        let node = &t.nodes[id];
        assert_eq!(*id, UiNodeId::Link { object: text, link });
        assert_eq!(node.role, UiRole::Link);
        assert_eq!(node.label, name);
        assert_eq!(
            node.bounds.map(f64::to_bits),
            glyph_bounds(&p, link).map(f64::to_bits)
        );
        assert!(node.children.is_empty());
        assert_eq!(node.value, UiSemanticValue::None);
        assert_eq!(node.actions, [UiAccessAction::Focus, UiAccessAction::Press]);
        assert!(!node.disabled);
    }
}

#[test]
fn link_identities_survive_relayout_and_removed_links_leave_the_update() {
    let (mut w, mut r, _, _, text) = fixture();
    two_links(&mut w, text);
    let first = tree(&w, &mut r);
    let ids = children(&first, text);
    assert_eq!(ids.len(), 2);
    request(&w, &mut r, ids[0], UiAccessAction::Focus);
    r.semantic_update(&w, &layout(&w, &r)).unwrap();
    let rect = w
        .published_components()
        .find_map(|c| match &c.desc {
            ComponentDesc::RectTransform(rect) if c.object == text => Some((c.handle, *rect)),
            _ => None,
        })
        .unwrap();
    w.set_component(
        rect.0,
        ComponentDesc::RectTransform(RectTransform {
            size_delta: Vec2::new(60., 60.),
            ..rect.1
        }),
    )
    .unwrap();
    r.layout.display_scale = 2.;
    let moved = tree(&w, &mut r);
    assert_eq!(children(&moved, text), ids);
    assert_ne!(
        first.nodes[&ids[0]].bounds.map(f64::to_bits),
        moved.nodes[&ids[0]].bounds.map(f64::to_bits)
    );
    edit_run(&mut w, text, |run| {
        run.spans.remove(0);
    });
    let update = r.semantic_update(&w, &layout(&w, &r)).unwrap();
    assert_eq!(update.removed, [ids[0]]);
    assert_eq!(update.focus, UiNodeId::Window);
    assert_eq!(r.focused_link(), None);
    assert_eq!(children(&tree(&w, &mut r), text), [ids[1]]);
    assert!(!request(&w, &mut r, ids[0], UiAccessAction::Press).consumed);
}

#[test]
fn assistive_link_focus_and_tab_share_the_keyboard_focus() {
    let (mut w, mut r, _, _, text) = fixture();
    two_links(&mut w, text);
    let ids = children(&tree(&w, &mut r), text);
    assert_eq!(ids.len(), 2);
    let response = request(&w, &mut r, ids[0], UiAccessAction::Focus);
    assert!(response.consumed);
    assert!(response.actions.is_empty());
    assert_eq!(r.focused_link(), Some((text, UiLinkId(9))));
    assert_eq!(tree(&w, &mut r).focus, ids[0]);
    r.semantic_update(&w, &layout(&w, &r)).unwrap();
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused_link(), Some((text, UiLinkId(2))));
    let update = r.semantic_update(&w, &layout(&w, &r)).unwrap();
    assert_eq!(update.focus, ids[1]);
    assert!(update.focus_changed);
}

#[test]
fn assistive_press_equals_enter_and_click_for_the_same_link() {
    let (w, mut r, fonts, _, text) = fixture();
    let ids = children(&tree(&w, &mut r), text);
    assert_eq!(ids.len(), 1);
    let pressed = request(&w, &mut r, ids[0], UiAccessAction::Press);
    assert_eq!(
        pressed.actions,
        [UiAction::LinkActivated {
            control: text,
            link: UiLinkId(7)
        }]
    );
    assert_eq!(key(&w, &mut r, UiKey::Enter), pressed);
    let p = plan(&w, &r, &fonts);
    pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 2)));
    button(&w, &mut r, true);
    assert_eq!(button(&w, &mut r, false), pressed);
    assert!(!request(&w, &mut r, ids[0], UiAccessAction::Toggle).consumed);
    assert!(!request(&w, &mut r, UiNodeId::Object(text), UiAccessAction::Press).consumed);
}

#[test]
fn disabled_paragraph_links_are_not_exposed_or_actionable() {
    let (mut w, mut r, _, _, text) = fixture();
    let ids = children(&tree(&w, &mut r), text);
    assert_eq!(ids.len(), 1);
    r.controls.get_mut(&text).unwrap().enabled = false;
    assert!(children(&tree(&w, &mut r), text).is_empty());
    assert!(!request(&w, &mut r, ids[0], UiAccessAction::Focus).consumed);
    r.controls.get_mut(&text).unwrap().enabled = true;
    edit_run(&mut w, text, |run| run.enabled = false);
    assert!(children(&tree(&w, &mut r), text).is_empty());
    assert!(!request(&w, &mut r, ids[0], UiAccessAction::Press).consumed);
}

#[test]
fn paragraph_without_links_keeps_the_complete_existing_snapshot() {
    let (mut w, mut r, _, root, text) = fixture();
    edit_run(&mut w, text, |run| run.spans.clear());
    let expected = UiSemanticTree {
        nodes: BTreeMap::from([
            (
                UiNodeId::Window,
                plain(
                    UiNodeId::Window,
                    UiRole::Window,
                    "",
                    [0., 0., 100., 100.],
                    vec![UiNodeId::Object(root)],
                    false,
                ),
            ),
            (
                UiNodeId::Object(root),
                plain(
                    UiNodeId::Object(root),
                    UiRole::Group,
                    "",
                    [0., 0., 100., 100.],
                    vec![UiNodeId::Object(text)],
                    false,
                ),
            ),
            (
                UiNodeId::Object(text),
                plain(
                    UiNodeId::Object(text),
                    UiRole::Text,
                    "AA AA",
                    [35., 20., 65., 80.],
                    vec![],
                    true,
                ),
            ),
        ]),
        focus: UiNodeId::Window,
    };
    assert_eq!(tree(&w, &mut r), expected);
    assert_eq!(
        format!("{:?}", tree(&w, &mut r)).as_bytes(),
        format!("{expected:?}").as_bytes()
    );
}
fn plain(
    id: UiNodeId,
    role: UiRole,
    label: &str,
    bounds: [f64; 4],
    children: Vec<UiNodeId>,
    disabled: bool,
) -> UiSemanticNode {
    UiSemanticNode {
        id,
        role,
        label: label.into(),
        bounds,
        children,
        disabled,
        actions: vec![],
        value: UiSemanticValue::None,
    }
}
