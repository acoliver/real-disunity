#![cfg(feature = "ui")]
#[path = "ui_links/edges.rs"]
mod edges;
#[path = "ui_links/text_fields.rs"]
mod text_fields;
mod ui_font;
use glam::{UVec2, Vec2};
use std::sync::Arc;
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, PointerButton, PointerEvent, World,
    WorldSettings,
};

fn rect(w: &mut World, parent: Option<ObjectHandle>, size: Vec2) -> ObjectHandle {
    let o = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(
        o,
        ComponentDesc::RectTransform(RectTransform {
            size_delta: size,
            ..Default::default()
        }),
    )
    .unwrap();
    w.set_parent(o, parent, ParentSpace::KeepLocal).unwrap();
    o
}
fn fixture() -> (World, UiRouter, Arc<FontBook>, ObjectHandle, ObjectHandle) {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let fonts = Arc::new(fonts);
    let mut w = World::new(WorldSettings::default()).unwrap();
    let root = rect(&mut w, None, Vec2::ZERO);
    w.add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let text = rect(&mut w, Some(root), Vec2::new(30., 60.));
    w.add_component(
        text,
        ComponentDesc::Text(TextRun {
            text: "AA AA".into(),
            font_size: 20.,
            line_height: Some(25.),
            spans: vec![TextSpan {
                range: 0..5,
                color: [1.; 4],
                link: Some(UiLinkId(7)),
            }],
            ..TextRun::new(font)
        }),
    )
    .unwrap();
    let mut r = UiRouter::with_fonts(fonts.clone());
    r.controls.insert(text, UiControl::text());
    r.link_styles.insert(
        (text, UiLinkId(7)),
        UiStyle {
            normal: [0.2, 0.3, 0.4, 1.],
            hovered: [0.3, 0.4, 0.5, 1.],
            focused: [0.4, 0.5, 0.6, 1.],
            pressed: [0.5, 0.6, 0.7, 1.],
            ..Default::default()
        },
    );
    (w, r, fonts, root, text)
}
fn layout(w: &World, r: &UiRouter) -> UiLayout {
    w.ui_layout_with(UVec2::splat(100), &r.layout).unwrap()
}
fn pointer(w: &World, r: &mut UiRouter, event: PointerEvent) -> UiResponse {
    r.pointer(w, &layout(w, r), event).unwrap()
}
fn button(w: &World, r: &mut UiRouter, pressed: bool) -> UiResponse {
    pointer(
        w,
        r,
        PointerEvent::Button {
            button: PointerButton::Left,
            pressed,
        },
    )
}
fn key(w: &World, r: &mut UiRouter, key: UiKey) -> UiResponse {
    let l = layout(w, r);
    r.key(w, &l, key, false, false).unwrap();
    r.key(w, &l, key, true, false).unwrap()
}
fn plan(w: &World, r: &UiRouter, fonts: &FontBook) -> contribution::UiRenderPlan {
    let mut p = contribution::extract_ui_layout(w, fonts, layout(w, r)).unwrap();
    r.style_plan(w, &mut p).unwrap();
    p
}
fn glyph_point(p: &contribution::UiRenderPlan, index: usize) -> [f64; 2] {
    let q = &p.texts[0].quads[index];
    let x = (q.corners[0][0] + q.corners[2][0]) * 0.5;
    let y = (q.corners[0][1] + q.corners[2][1]) * 0.5;
    [f64::from(x), 100. - f64::from(y)]
}
fn assert_tint(p: &contribution::UiRenderPlan, color: [f32; 4]) {
    assert!(
        p.texts[0]
            .quads
            .iter()
            .all(|q| q.color.map(f32::to_bits) == color.map(f32::to_bits))
    );
    assert!(
        p.graphics
            .iter()
            .flat_map(|graphic| &graphic.graphic.quads)
            .all(|q| q.color.map(f32::to_bits) == color.map(f32::to_bits))
    );
}

fn edit_run(w: &mut World, text: ObjectHandle, edit: impl FnOnce(&mut TextRun)) {
    let (handle, mut run) = w
        .published_components()
        .find_map(|c| match &c.desc {
            ComponentDesc::Text(t) if c.object == text => Some((c.handle, t.clone())),
            _ => None,
        })
        .unwrap();
    edit(&mut run);
    w.set_component(handle, ComponentDesc::Text(run)).unwrap();
}
fn two_links(w: &mut World, text: ObjectHandle) {
    edit_run(w, text, |run| {
        run.spans = vec![
            TextSpan {
                range: 0..2,
                color: [1.; 4],
                link: Some(UiLinkId(9)),
            },
            TextSpan {
                range: 3..5,
                color: [1.; 4],
                link: Some(UiLinkId(2)),
            },
        ];
    });
}
#[test]
fn wrapped_link_clicks_both_lines_and_tints_the_whole_span() {
    let (w, mut r, fonts, _, text) = fixture();
    assert_tint(&plan(&w, &r, &fonts), [0.2, 0.3, 0.4, 1.]);
    for index in [0, 2] {
        let p = plan(&w, &r, &fonts);
        pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, index)));
        let expected = if r.focused().is_some() {
            [0.4, 0.5, 0.6, 1.]
        } else {
            [0.3, 0.4, 0.5, 1.]
        };
        assert_tint(&plan(&w, &r, &fonts), expected);
        button(&w, &mut r, true);
        assert_tint(&plan(&w, &r, &fonts), [0.5, 0.6, 0.7, 1.]);
        assert_eq!(
            button(&w, &mut r, false).actions,
            [UiAction::LinkActivated {
                control: text,
                link: UiLinkId(7)
            }]
        );
        assert!(button(&w, &mut r, false).actions.is_empty());
        assert_tint(&plan(&w, &r, &fonts), [0.4, 0.5, 0.6, 1.]);
    }
}
#[test]
fn underlines_are_one_solid_quad_below_each_baseline() {
    let (w, r, fonts, _, text) = fixture();
    let p = plan(&w, &r, &fonts);
    assert_eq!(p.graphics.len(), 1);
    let graphic = &p.graphics[0];
    assert_eq!(graphic.graphic.object, text);
    assert_eq!(graphic.graphic.texture, None);
    assert_eq!(graphic.graphic.quads.len(), 2);
    let rect = p.layout.rect(text).unwrap();
    let run = w
        .published_components()
        .find_map(|c| match &c.desc {
            ComponentDesc::Text(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let shaped = fonts.layout(run, rect.size).unwrap();
    for (q, line) in graphic.graphic.quads.iter().zip(&shaped.lines) {
        let origin = rect.corners[0];
        assert_eq!(q.corners[0][0].to_bits(), origin.x.to_bits());
        assert_eq!(q.corners[1][0].to_bits(), (origin.x + 22.).to_bits());
        assert!(q.corners[2][1] < origin.y + line.baseline);
        assert_eq!(
            (q.corners[2][1] - q.corners[0][1]).to_bits(),
            1.25_f32.to_bits()
        );
    }
}
#[test]
fn tab_reading_order_reverse_and_enter_edges_include_links_between_controls() {
    let (mut w, mut r, fonts, root, text) = fixture();
    let before = root;
    let after = rect(&mut w, Some(root), Vec2::ONE);
    r.controls.insert(before, UiControl::button());
    r.controls.insert(after, UiControl::button());
    two_links(&mut w, text);
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(before));
    for link in [UiLinkId(9), UiLinkId(2)] {
        key(&w, &mut r, UiKey::Tab);
        assert_eq!(r.focused_link(), Some((text, link)));
        assert_eq!(
            key(&w, &mut r, UiKey::Enter).actions,
            [UiAction::LinkActivated {
                control: text,
                link
            }]
        );
        let l = layout(&w, &r);
        assert!(
            r.key(&w, &l, UiKey::Enter, true, true)
                .unwrap()
                .actions
                .is_empty()
        );
        assert!(
            r.key(&w, &l, UiKey::Enter, true, false)
                .unwrap()
                .actions
                .is_empty()
        );
    }
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(after));
    key(&w, &mut r, UiKey::Shift);
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused_link(), Some((text, UiLinkId(2))));
    assert_eq!(plan(&w, &r, &fonts).texts[0].quads.len(), 4);
}
#[test]
fn modal_blocks_links_and_cancellation_invalidates_capture() {
    let (mut w, mut r, fonts, root, _) = fixture();
    let p = plan(&w, &r, &fonts);
    let at = glyph_point(&p, 0);
    pointer(&w, &mut r, PointerEvent::Moved(at));
    button(&w, &mut r, true);
    let modal = rect(&mut w, Some(root), Vec2::ONE);
    r.modals.insert(modal);
    assert!(button(&w, &mut r, false).actions.is_empty());
    assert!(key(&w, &mut r, UiKey::Tab).consumed);
    assert_eq!(r.focused(), None);
    r.modals.clear();
    pointer(&w, &mut r, PointerEvent::Moved(at));
    button(&w, &mut r, true);
    pointer(&w, &mut r, PointerEvent::Cancel);
    assert!(button(&w, &mut r, false).actions.is_empty());
}
#[test]
fn viewport_clips_glyph_hits_and_focus_reveals_the_link_not_the_paragraph() {
    let (mut w, mut r, fonts, root, text) = fixture();
    let view = rect(&mut w, Some(root), Vec2::new(30., 20.));
    w.set_parent(text, Some(view), ParentSpace::KeepLocal)
        .unwrap();
    r.layout.scrolls.insert(view, UiScrollView::new(text));
    let p = plan(&w, &r, &fonts);
    pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 2)));
    assert!(button(&w, &mut r, true).actions.is_empty());
    assert!(button(&w, &mut r, false).actions.is_empty());
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused_link(), Some((text, UiLinkId(7))));
    // This link is taller than the viewport: reveal its first reading segment.
    assert_eq!(r.layout.scrolls[&view].offset(), Vec2::ZERO);
    edit_run(&mut w, text, |run| run.spans[0].range = 3..5);
    pointer(&w, &mut r, PointerEvent::Cancel);
    key(&w, &mut r, UiKey::Tab);
    assert!(r.layout.scrolls[&view].offset().y > 0.);
    let p = plan(&w, &r, &fonts);
    let at = glyph_point(&p, 2);
    pointer(&w, &mut r, PointerEvent::Moved(at));
    button(&w, &mut r, true);
    assert_eq!(
        button(&w, &mut r, false).actions,
        [UiAction::LinkActivated {
            control: text,
            link: UiLinkId(7)
        }]
    );
}

#[test]
fn semantic_tree_keeps_containers_and_preserves_link_keyboard_focus() {
    let (w, mut r, _, root, text) = fixture();
    key(&w, &mut r, UiKey::Tab);
    let tree = r.semantic_tree(&w, &layout(&w, &r)).unwrap();
    assert_eq!(tree.nodes[&UiNodeId::Object(root)].role, UiRole::Group);
    assert_eq!(tree.nodes[&UiNodeId::Object(text)].role, UiRole::Text);
    assert_eq!(tree.focus, UiNodeId::Object(text));
    assert_eq!(r.focused_link(), Some((text, UiLinkId(7))));
    assert_eq!(
        key(&w, &mut r, UiKey::Enter).actions,
        [UiAction::LinkActivated {
            control: text,
            link: UiLinkId(7),
        }]
    );
}
