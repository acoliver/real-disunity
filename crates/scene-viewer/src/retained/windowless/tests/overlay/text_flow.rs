use super::*;
use glam::Vec2;
use worldrunner_world_simulation::ui::{
    Canvas, FontBook, RectTransform, TextRun, TextSpan, UiLimits, UiLinkId,
};
use worldrunner_world_simulation::{ComponentDesc, ParentSpace};
#[path = "../../../../../../world-simulation/tests/ui_font/mod.rs"]
mod ui_font;

fn paragraph_plan(link: Option<UiLinkId>) -> UiRenderPlan {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    world
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let panel = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(panel, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(
            panel,
            ComponentDesc::RectTransform(RectTransform {
                size_delta: Vec2::new(28., 56.),
                ..Default::default()
            }),
        )
        .unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    world
        .add_component(
            panel,
            ComponentDesc::Text(TextRun {
                text: "AA AA\nAA AA".into(),
                font_size: 10.,
                spans: vec![
                    TextSpan {
                        link,
                        range: 0..5,
                        color: [1., 0., 0., 1.],
                    },
                    TextSpan {
                        link: None,
                        range: 6..11,
                        color: [0., 1., 0., 1.],
                    },
                ],
                line_height: Some(14.),
                paragraph_spacing: 4.,
                ..TextRun::new(font)
            }),
        )
        .unwrap();
    worldrunner_render_plan::ui::extract_ui(&world, &fonts, [64, 64]).unwrap()
}

#[test]
fn paragraph_spans_draw_exact_regions_without_overlapping_lines() {
    let plan = paragraph_plan(None);
    assert_eq!(plan.texts[0].quads.len(), 8);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events).1;
    apply(&mut run, plan);
    let image = capture(&mut run, &events).1;
    for (base, expected) in [
        (52, [255, 0, 0, 255]),
        (38, [255, 0, 0, 255]),
        (20, [0, 255, 0, 255]),
        (6, [0, 255, 0, 255]),
    ] {
        for y in base..base + 2 {
            for x in 18..20 {
                assert_eq!(pixel(&image, x, y), expected, "paragraph pixel {x},{y}");
            }
        }
    }
    for y in [14..20, 28..38, 46..52] {
        for y in y {
            for x in 18..46 {
                assert_eq!(
                    pixel(&image, x, y),
                    pixel(&baseline, x, y),
                    "line gap {x},{y}"
                );
            }
        }
    }
    run.shutdown().unwrap();
}

#[test]
fn link_underlines_use_existing_overlay_graphics() {
    let plan = paragraph_plan(Some(UiLinkId(7)));
    assert_eq!(plan.graphics.len(), 1);
    assert_eq!(plan.graphics[0].graphic.quads.len(), 2);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events).1;
    apply(&mut run, plan);
    let image = capture(&mut run, &events).1;
    for y in [50, 36] {
        for x in 18..29 {
            assert_eq!(pixel(&image, x, y), [255, 0, 0, 255], "underline {x},{y}");
            assert_eq!(pixel(&image, x, y - 1), pixel(&baseline, x, y - 1));
        }
    }
    run.shutdown().unwrap();
}
