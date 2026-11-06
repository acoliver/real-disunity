#![cfg(feature = "ui")]
mod ui_font;
use glam::{DVec3, Vec2, Vec3};
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, ResolvedView, ViewConvention, Viewport,
};
use worldrunner_world_simulation::ui::{
    FontBook, TextRun, UiError, UiGeometry, UiLimits, UiVertex,
    annotations::{
        Annotation, AnnotationDepth, AnnotationPoint, AnnotationPrimitive, extract_annotations,
    },
};

fn view(width: u32, eye: DVec3) -> ResolvedView {
    worldrunner_camera::resolve(&CameraState {
        pose: CameraPose {
            eye_meters: eye,
            forward: -eye,
            up: DVec3::Y,
        },
        lens: CameraLens::Perspective {
            vertical_fov_radians: 1.,
            near_meters: 0.1,
            far_meters: 100.,
        },
        viewport: Viewport {
            width,
            height: width,
        },
        convention: ViewConvention::RIGHT_HANDED,
    })
    .unwrap()
}
fn arrow() -> Annotation {
    Annotation {
        primitive: AnnotationPrimitive::Arrow {
            from: AnnotationPoint::Screen(Vec2::new(8., 16.)),
            to: AnnotationPoint::Screen(Vec2::new(40., 16.)),
            width: 4.,
            head_length: 12.,
            head_width: 16.,
            colour: [1., 0., 0., 1.],
        },
        draw_order: (2, 3, 4),
        clip_rect: Some([10, 10, 20, 20]),
        depth: AnnotationDepth::AlwaysVisible,
    }
}
#[test]
fn arrow_head_is_a_triangle_and_preserves_sorting_and_clip() {
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan = extract_annotations(&[arrow()], &fonts, &view(64, DVec3::Z * 5.)).unwrap();
    assert_eq!(plan.draws.len(), 3);
    let head = plan.draws.last().unwrap();
    assert_eq!(
        head.corners[..3],
        [[28., 24., 0.], [28., 8., 0.], [40., 16., 0.]]
    );
    assert!(head.triangle_uv.is_some());
    assert_eq!(head.clip_rect, [10, 10, 20, 20]);
    assert_eq!(
        (head.sorting_layer, head.sorting_order, head.hierarchy_order),
        (2, 3, 4)
    );
    let short = Annotation {
        primitive: AnnotationPrimitive::Arrow {
            from: AnnotationPoint::Screen(Vec2::splat(1e-8)),
            to: AnnotationPoint::Screen(Vec2::X),
            width: 4.,
            head_length: 12.,
            head_width: 16.,
            colour: [1.; 4],
        },
        ..arrow()
    };
    let clamped = extract_annotations(&[short], &fonts, &view(64, DVec3::Z * 5.)).unwrap();
    assert_eq!(clamped.draws.len(), 1, "an oversized head has no shaft");
}
#[test]
fn valid_triangle_is_retained_by_geometry_extraction() {
    let vertices = [[0., 0., 0.], [10., 0., 0.], [0., 10., 0.]].map(|position| UiVertex {
        position,
        uv: [0.; 2],
        color: [1.; 4],
    });
    let geometry = UiGeometry::new(vertices.to_vec(), vec![0, 1, 2]).unwrap();
    assert_eq!(geometry.quads().triangles, [vertices]);
}
#[test]
fn world_anchor_tracks_orbit_resize_and_clips_behind_camera() {
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let mut annotation = arrow();
    let point = Vec3::X;
    annotation.primitive = AnnotationPrimitive::Circle {
        centre: AnnotationPoint::World(point),
        radius: 4.,
        thickness: None,
        colour: [1.; 4],
        segments: 16,
    };
    for (width, eye) in [
        (64, DVec3::Z * 5.),
        (128, DVec3::Z * 5.),
        (64, DVec3::X * 5.),
    ] {
        let view = view(width, eye);
        let projected = view.world_to_screen(point).unwrap();
        let plan = extract_annotations(&[annotation.clone()], &fonts, &view).unwrap();
        let actual = Vec3::from(plan.draws[0].corners[0]);
        let expected = Vec3::new(
            projected.x,
            f32::from(u16::try_from(width).unwrap()) - projected.y,
            0.,
        );
        assert!(actual.abs_diff_eq(expected, 1e-5));
    }
    for point in [Vec3::Z * 10., Vec3::X * 100., Vec3::Z * 5.] {
        annotation.primitive = AnnotationPrimitive::Circle {
            centre: AnnotationPoint::World(point),
            radius: 4.,
            thickness: None,
            colour: [1.; 4],
            segments: 16,
        };
        assert!(
            extract_annotations(&[annotation.clone()], &fonts, &view(64, DVec3::Z * 5.))
                .unwrap()
                .draws
                .is_empty()
        );
    }
}
#[test]
fn screen_label_keeps_pixel_geometry_across_resize() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let mut annotation = arrow();
    annotation.primitive = AnnotationPrimitive::Label {
        anchor: AnnotationPoint::Screen(Vec2::splat(20.)),
        run: TextRun {
            text: "A".into(),
            font_size: 20.,
            ..TextRun::new(font)
        },
        alignment: Vec2::splat(0.5),
    };
    let small =
        extract_annotations(&[annotation.clone()], &fonts, &view(64, DVec3::Z * 5.)).unwrap();
    let large = extract_annotations(&[annotation], &fonts, &view(128, DVec3::Z * 5.)).unwrap();
    assert!(!small.draws.is_empty());
    assert_eq!(small.draws[0].corners, large.draws[0].corners);
    assert_eq!(small.coverage, large.coverage);
}
#[test]
fn annotation_budget_and_unsupported_depth_fail_explicitly() {
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let view = view(64, DVec3::Z * 5.);
    let mut annotation = arrow();
    annotation.depth = AnnotationDepth::DepthTested;
    assert_eq!(
        extract_annotations(&[annotation], &fonts, &view),
        Err(UiError::UnsupportedAnnotationDepth)
    );
    let mut annotation = arrow();
    annotation.primitive = AnnotationPrimitive::Circle {
        centre: AnnotationPoint::Screen(Vec2::splat(20.)),
        radius: 8.,
        thickness: Some(2.),
        colour: [1.; 4],
        segments: 257,
    };
    assert_eq!(
        extract_annotations(&[annotation], &fonts, &view),
        Err(UiError::AnnotationLimit)
    );
    assert_eq!(
        extract_annotations(&vec![arrow(); 8192], &fonts, &view),
        Err(UiError::AnnotationLimit)
    );
}

#[test]
fn line_width_is_in_pixels_and_invalid_geometry_is_rejected() {
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let view = view(64, DVec3::Z * 5.);
    let line = |width| Annotation {
        primitive: AnnotationPrimitive::Line {
            from: AnnotationPoint::Screen(Vec2::new(8., 16.)),
            to: AnnotationPoint::Screen(Vec2::new(40., 16.)),
            width,
            colour: [1.; 4],
        },
        ..arrow()
    };
    let plan = extract_annotations(&[line(4.)], &fonts, &view).unwrap();
    assert_eq!(plan.draws.len(), 2);
    assert_eq!(
        plan.draws[0].corners[..3],
        [[8., 14., 0.], [40., 14., 0.], [40., 18., 0.]]
    );
    for width in [0., -1., f32::NAN, f32::INFINITY] {
        assert_eq!(
            extract_annotations(&[line(width)], &fonts, &view),
            Err(UiError::InvalidAnnotation)
        );
    }
}
