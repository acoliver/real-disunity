#![cfg(feature = "ui")]
mod ui_font;
use glam::{DVec3, Vec2, Vec3};
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, ResolvedView, ViewConvention, Viewport,
};
use worldrunner_world_simulation::ui::{
    FontBook, TextRun, UiError, UiLimits,
    annotations::{
        Annotation, AnnotationDepth, AnnotationPoint, AnnotationPrimitive, extract_annotations,
    },
};
fn view() -> ResolvedView {
    worldrunner_camera::resolve(&CameraState {
        pose: CameraPose {
            eye_meters: DVec3::Z * 5.,
            forward: -DVec3::Z,
            up: DVec3::Y,
        },
        lens: CameraLens::Perspective {
            vertical_fov_radians: 1.,
            near_meters: 0.1,
            far_meters: 100.,
        },
        viewport: Viewport {
            width: 64,
            height: 64,
        },
        convention: ViewConvention::RIGHT_HANDED,
    })
    .unwrap()
}
fn line(from: AnnotationPoint, to: AnnotationPoint) -> Annotation {
    Annotation {
        primitive: AnnotationPrimitive::Line {
            from,
            to,
            width: 4.,
            colour: [1.; 4],
        },
        draw_order: (0, 0, 0),
        clip_rect: None,
        depth: AnnotationDepth::DepthTested,
    }
}
#[test]
fn world_depth_tested_line_preserves_projected_endpoint_depths() {
    let view = view();
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let a = Vec3::new(-1., 0., 1.);
    let b = Vec3::new(1., 0., -1.);
    let annotation = line(AnnotationPoint::World(a), AnnotationPoint::World(b));
    let plan = extract_annotations(std::slice::from_ref(&annotation), &fonts, &view).unwrap();
    assert_eq!(plan.draws.len(), 2);
    assert!(plan.draws.iter().all(|draw| draw.depth_test));
    assert_eq!(
        plan.draws[0].corners[0][2].to_bits(),
        view.world_to_screen(a).unwrap().z.to_bits()
    );
    assert_eq!(
        plan.draws[0].corners[1][2].to_bits(),
        view.world_to_screen(b).unwrap().z.to_bits()
    );
    assert_ne!(
        plan.draws[0].corners[0][2].to_bits(),
        plan.draws[0].corners[1][2].to_bits()
    );
    let always = extract_annotations(
        &[Annotation {
            depth: AnnotationDepth::AlwaysVisible,
            ..annotation
        }],
        &fonts,
        &view,
    )
    .unwrap();
    assert!(always.draws.iter().all(|draw| !draw.depth_test));
    for (tested, always) in plan.draws.iter().zip(&always.draws) {
        for (a, b) in tested.corners.iter().zip(always.corners) {
            assert_eq!(a[..2], b[..2]);
        }
    }
}
#[test]
fn depth_tested_screen_or_mixed_anchors_are_rejected() {
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    for to in [
        AnnotationPoint::Screen(Vec2::splat(20.)),
        AnnotationPoint::World(Vec3::ZERO),
    ] {
        assert_eq!(
            extract_annotations(
                &[line(AnnotationPoint::Screen(Vec2::ZERO), to)],
                &fonts,
                &view()
            ),
            Err(UiError::UnsupportedAnnotationDepth)
        );
    }
}
#[test]
fn clipped_depth_tested_world_anchors_emit_no_draws() {
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    for outside in [Vec3::Z * 5., Vec3::Z * 10., Vec3::X * 100.] {
        let annotation = line(
            AnnotationPoint::World(outside),
            AnnotationPoint::World(Vec3::ZERO),
        );
        assert!(
            extract_annotations(&[annotation], &fonts, &view())
                .unwrap()
                .draws
                .is_empty()
        );
    }
}
#[test]
fn depth_tested_circle_and_label_keep_the_world_anchor_depth() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let anchor = AnnotationPoint::World(Vec3::ZERO);
    let circle = AnnotationPrimitive::Circle {
        centre: anchor,
        radius: 8.,
        thickness: None,
        colour: [1.; 4],
        segments: 16,
    };
    let label = AnnotationPrimitive::Label {
        anchor,
        run: TextRun {
            text: "A".into(),
            ..TextRun::new(font)
        },
        alignment: Vec2::splat(0.5),
    };
    let expected = view().world_to_screen(Vec3::ZERO).unwrap().z.to_bits();
    for primitive in [circle, label] {
        let annotation = Annotation {
            primitive,
            ..line(anchor, anchor)
        };
        let plan = extract_annotations(&[annotation], &fonts, &view()).unwrap();
        assert!(!plan.draws.is_empty());
        assert!(
            plan.draws
                .iter()
                .all(|draw| draw.depth_test
                    && draw.corners.iter().all(|c| c[2].to_bits() == expected))
        );
    }
}
