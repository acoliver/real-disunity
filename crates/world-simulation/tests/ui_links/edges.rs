#[test]
fn live_2d_text_append_includes_generated_underlines() {
    use worldrunner_world_simulation::presentation::extraction::{
        SpriteTextures, append_ui, extract_world,
    };
    let (mut world, router, fonts, _, _) = fixture();
    let camera = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            camera,
            ComponentDesc::Camera(worldrunner_world_simulation::presentation::Camera::default()),
        )
        .unwrap();
    let ui = plan(&world, &router, &fonts);
    let mut frame = extract_world(&world, &SpriteTextures::default(), [100, 100]).unwrap();
    append_ui(&mut frame, ui).unwrap();
    assert_eq!(frame.quads.len(), 6);
    assert_eq!(
        frame.quads.iter().filter(|q| q.texture.is_none()).count(),
        2
    );
}

use super::*;

#[test]
fn capture_can_release_on_another_line_but_not_another_link() {
    let (mut w, mut r, fonts, _, text) = fixture();
    let p = plan(&w, &r, &fonts);
    pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 0)));
    button(&w, &mut r, true);
    pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 3)));
    assert_eq!(
        button(&w, &mut r, false).actions,
        [UiAction::LinkActivated {
            control: text,
            link: UiLinkId(7)
        }]
    );
    two_links(&mut w, text);
    let p = plan(&w, &r, &fonts);
    pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 0)));
    button(&w, &mut r, true);
    pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 3)));
    assert!(button(&w, &mut r, false).actions.is_empty());
}
#[test]
fn unlinked_text_and_paragraph_whitespace_do_not_consume_hits_or_focus_tints() {
    let (mut w, mut r, fonts, _, text) = fixture();
    edit_run(&mut w, text, |run| run.spans[0].range = 3..5);
    let p = plan(&w, &r, &fonts);
    assert!(!pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 0))).consumed);
    assert!(!button(&w, &mut r, true).consumed);
    assert!(button(&w, &mut r, false).actions.is_empty());
    assert!(!pointer(&w, &mut r, PointerEvent::Moved([64., 75.])).consumed);
    key(&w, &mut r, UiKey::Tab);
    let p = plan(&w, &r, &fonts);
    assert_eq!(
        p.texts[0].quads[0].color.map(f32::to_bits),
        [1_f32; 4].map(f32::to_bits)
    );
    assert_eq!(
        p.texts[0].quads[2].color.map(f32::to_bits),
        [0.4_f32, 0.5, 0.6, 1.].map(f32::to_bits)
    );
}
#[test]
fn disabled_links_block_hits_skip_tab_and_use_authored_disabled_tint() {
    let (w, mut r, fonts, _, text) = fixture();
    r.controls.get_mut(&text).unwrap().enabled = false;
    r.link_styles
        .get_mut(&(text, UiLinkId(7)))
        .unwrap()
        .disabled = [0.1, 0.1, 0.1, 1.];
    let p = plan(&w, &r, &fonts);
    assert_tint(&p, [0.1, 0.1, 0.1, 1.]);
    assert!(pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 0))).consumed);
    button(&w, &mut r, true);
    assert!(button(&w, &mut r, false).actions.is_empty());
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), None);
}
#[test]
fn canvas_group_flags_apply_to_link_glyphs() {
    let (mut w, mut r, fonts, root, _) = fixture();
    w.add_component(
        root,
        ComponentDesc::CanvasGroup(CanvasGroup {
            flags: CanvasGroupFlags {
                interactable: false,
                blocks_raycasts: false,
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .unwrap();
    let p = plan(&w, &r, &fonts);
    assert!(!pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 0))).consumed);
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused_link(), None);
}
#[test]
fn renderer_clip_bounds_also_reject_hidden_link_parts() {
    let (mut w, mut r, fonts, _, text) = fixture();
    w.add_component(
        text,
        ComponentDesc::CanvasRenderer(CanvasRenderer {
            clip_rect: Some([-10., -50., 5., 100.]),
            ..Default::default()
        }),
    )
    .unwrap();
    let p = plan(&w, &r, &fonts);
    let mut at = glyph_point(&p, 0);
    at[0] = 37.;
    assert!(!pointer(&w, &mut r, PointerEvent::Moved(at)).consumed);
    button(&w, &mut r, true);
    assert!(button(&w, &mut r, false).actions.is_empty());
    at[0] = 42.;
    assert!(pointer(&w, &mut r, PointerEvent::Moved(at)).consumed);
    button(&w, &mut r, true);
    assert_eq!(button(&w, &mut r, false).actions.len(), 1);
    assert_eq!(p.graphics[0].clip_rect, p.texts[0].clip_rect);
}
#[test]
fn nested_scroll_clips_intersect_for_link_hits() {
    let (mut w, mut r, fonts, root, text) = fixture();
    let outer = rect(&mut w, Some(root), Vec2::new(10., 20.));
    let inner = rect(&mut w, Some(outer), Vec2::new(30., 20.));
    w.set_parent(text, Some(inner), ParentSpace::KeepLocal)
        .unwrap();
    r.layout.scrolls.insert(outer, UiScrollView::new(inner));
    r.layout.scrolls.insert(inner, UiScrollView::new(text));
    let p = plan(&w, &r, &fonts);
    let mut at = glyph_point(&p, 0);
    at[0] = 56.;
    assert!(!pointer(&w, &mut r, PointerEvent::Moved(at)).consumed);
    button(&w, &mut r, true);
    assert!(button(&w, &mut r, false).actions.is_empty());
    at[0] = 50.;
    assert!(pointer(&w, &mut r, PointerEvent::Moved(at)).consumed);
    button(&w, &mut r, true);
    assert_eq!(button(&w, &mut r, false).actions.len(), 1);
}
#[test]
fn text_rectangle_clips_overflowing_link_glyphs_and_underlines() {
    let (mut w, mut r, fonts, _, text) = fixture();
    edit_run(&mut w, text, |run| {
        run.horizontal_overflow = HorizontalOverflow::Overflow;
        run.text = "AAAAA".into();
    });
    let p = plan(&w, &r, &fonts);
    assert_eq!(p.texts[0].quads.len(), 3);
    assert!(!pointer(&w, &mut r, PointerEvent::Moved([70., 30.])).consumed);
    for q in &p.graphics[0].graphic.quads {
        assert!(q.corners.iter().all(|c| c[0] <= 65.));
    }
}
#[test]
fn link_focus_restores_after_modal_and_disappears_when_text_changes() {
    let (mut w, mut r, fonts, root, text) = fixture();
    two_links(&mut w, text);
    key(&w, &mut r, UiKey::Tab);
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused_link(), Some((text, UiLinkId(2))));
    let modal = rect(&mut w, Some(root), Vec2::ONE);
    r.controls.insert(modal, UiControl::button());
    r.modals.insert(modal);
    key(&w, &mut r, UiKey::Tab);
    assert_eq!(r.focused(), Some(modal));
    r.modals.clear();
    key(&w, &mut r, UiKey::Shift);
    assert_eq!(r.focused_link(), Some((text, UiLinkId(2))));
    edit_run(&mut w, text, |run| run.spans.pop().map(|_| ()).unwrap());
    assert!(key(&w, &mut r, UiKey::Enter).actions.is_empty());
    assert_eq!(r.focused(), None);
    assert_eq!(plan(&w, &r, &fonts).graphics.len(), 1);
}
#[test]
fn link_ids_invalidate_the_layout_cache_and_duplicate_ids_fail() {
    let (w, r, fonts, _, text) = fixture();
    let run = w
        .published_components()
        .find_map(|c| match &c.desc {
            ComponentDesc::Text(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap();
    let first = fonts.layout(&run, Vec2::new(30., 60.)).unwrap();
    let stats = fonts.cache_stats();
    for _ in 0..3 {
        plan(&w, &r, &fonts);
    }
    assert_eq!(fonts.cache_stats(), stats);
    let mut changed = run;
    changed.spans[0].link = Some(UiLinkId(8));
    let next = fonts.layout(&changed, Vec2::new(30., 60.)).unwrap();
    assert!(!Arc::ptr_eq(&first, &next));
    assert_eq!(next.glyphs[0].link, Some(UiLinkId(8)));
    changed.spans[0].range = 0..2;
    changed.spans.push(TextSpan {
        range: 3..5,
        color: [1.; 4],
        link: Some(UiLinkId(8)),
    });
    assert_eq!(
        fonts.layout(&changed, Vec2::new(30., 60.)),
        Err(UiError::InvalidText)
    );
    assert!(r.controls.contains_key(&text));
}
#[test]
fn rotated_pixel_snapped_links_use_the_extracted_glyph_rectangles() {
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
    let p = plan(&w, &r, &fonts);
    pointer(&w, &mut r, PointerEvent::Moved(glyph_point(&p, 2)));
    button(&w, &mut r, true);
    assert_eq!(
        button(&w, &mut r, false).actions,
        [UiAction::LinkActivated {
            control: text,
            link: UiLinkId(7)
        }]
    );
    let presentation = presentation::UiPresentation::from_plan(&p);
    assert_eq!(
        presentation
            .draws
            .iter()
            .filter(|d| d.kind == presentation::UiDrawKind::Graphic)
            .count(),
        2
    );
}
