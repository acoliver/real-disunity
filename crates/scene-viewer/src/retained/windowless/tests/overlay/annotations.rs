use worldrunner_world_simulation::ui::{
    FontBook, UiLimits,
    annotations::{
        Annotation, AnnotationDepth, AnnotationPoint, AnnotationPrimitive, extract_annotations,
    },
};

#[test]
fn filled_circle_and_ring_match_analytic_pixel_masks() {
    for thickness in [None, Some(4.)] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut run = overlay_host().windowless(&mut driver).unwrap();
        assert!(run.host().presented_view().is_none());
        let baseline = capture(&mut run, &events).1;
        let fonts = FontBook::new(UiLimits::default()).unwrap();
        let annotation = Annotation {
            primitive: AnnotationPrimitive::Circle {
                centre: AnnotationPoint::Screen(glam::Vec2::splat(32.)),
                radius: 12.,
                thickness,
                colour: [1., 0., 0., 1.],
                segments: 128,
            },
            draw_order: (0, 0, 0),
            clip_rect: None,
            depth: AnnotationDepth::AlwaysVisible,
        };
        let plan = extract_annotations(&[annotation], &fonts, run.host().presented_view().unwrap())
            .unwrap();
        run.host_mut()
            .apply(RetainedUpdate::Ui(std::sync::Arc::new(
                UiOverlay::from_presentation(&plan, &std::collections::BTreeMap::new()).unwrap(),
            )))
            .unwrap();
        let image = capture(&mut run, &events).1;
        assert_circle(&image, &baseline, thickness);
        run.shutdown().unwrap();
    }
}
fn assert_circle(image: &RgbaImage, baseline: &RgbaImage, thickness: Option<f32>) {
    let inner = thickness.map_or(0., |t| 12. - t);
    let mut count = 0u16;
    for y in 0u16..64 {
        for x in 0u16..64 {
            let distance =
                glam::Vec2::new(f32::from(x) + 0.5 - 32., f32::from(y) + 0.5 - 32.).length();
            let actual = pixel(image, u32::from(x), u32::from(y));
            if actual == [255, 0, 0, 255] {
                count += 1;
            }
            if distance < 11.5 && (inner == 0. || distance > inner + 0.5) {
                assert_eq!(actual, [255, 0, 0, 255], "interior {x},{y}");
            }
            if distance > 12.5 || distance < inner - 0.5 {
                assert_eq!(
                    actual,
                    pixel(baseline, u32::from(x), u32::from(y)),
                    "exterior {x},{y}"
                );
            }
        }
    }
    let area = std::f32::consts::PI * (144. - inner * inner);
    assert!(
        (f32::from(count) - area).abs() < area * 0.05,
        "count {count}, area {area}"
    );
}

use super::*;
use worldrunner_world_simulation::ComponentDesc;
use worldrunner_world_simulation::ui::{CanvasRenderer, UiGeometry, UiVertex};

#[test]
fn triangle_arrow_head_draws_exact_bytes_and_respects_clip() {
    let (mut world, object) = graphics::graphic_world();
    let vertices = [[-4., -8., 0.], [-4., 8., 0.], [12., 0., 0.]].map(|position| UiVertex {
        position,
        uv: [0.; 2],
        color: [1.; 4],
    });
    let component = world
        .published_components()
        .find(|c| c.object == object && matches!(c.desc, ComponentDesc::CanvasRenderer(_)))
        .unwrap()
        .handle;
    world
        .set_component(
            component,
            ComponentDesc::CanvasRenderer(CanvasRenderer {
                color: [1., 0., 0., 1.],
                geometry: Some(std::sync::Arc::new(
                    UiGeometry::new(vertices.to_vec(), vec![0, 1, 2]).unwrap(),
                )),
                ..CanvasRenderer::default()
            }),
        )
        .unwrap();
    let mut ui = graphics::extracted(&world);
    ui.graphics[0].clip_rect = [30, 0, 34, 64];
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events).1;
    apply(&mut run, ui);
    let image = capture(&mut run, &events).1;
    for y in 30..34 {
        for x in 30..36 {
            assert_eq!(pixel(&image, x, y), [255, 0, 0, 255], "{x},{y}");
        }
    }
    for (x, y) in [(29, 32), (40, 38), (44, 32)] {
        assert_eq!(pixel(&image, x, y), pixel(&baseline, x, y));
    }
    run.shutdown().unwrap();
}
