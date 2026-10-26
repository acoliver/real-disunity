#![cfg(feature = "ui")]

#[test]
fn scaler_pivot_and_camera_resize_have_matching_pixel_corners() {
    let mut w = world();
    let camera = w.create_object(ObjectDesc::default()).unwrap();
    for mode in [
        CanvasMode::ScreenSpaceOverlay,
        CanvasMode::ScreenSpaceCamera {
            camera,
            orthographic_size: 5.,
            plane_distance: 2.,
        },
    ] {
        let root = rect(
            &mut w,
            None,
            RectTransform {
                pivot: Vec2::new(0.25, 0.75),
                ..Default::default()
            },
        );
        w.add_component(
            root,
            ComponentDesc::Canvas(Canvas {
                mode,
                scale: CanvasScale::MatchViewport {
                    reference: Vec2::new(400., 300.),
                    match_height: 0.5,
                },
                ..Default::default()
            }),
        )
        .unwrap();
        for extent in [viewport(), UVec2::new(1600, 600)] {
            let out = w.ui_layout(extent).unwrap();
            let resolved = out.rect(root).unwrap();
            let factor = (extent.as_vec2() / Vec2::new(400., 300.))
                .element_product()
                .sqrt();
            assert!(resolved.size.abs_diff_eq(extent.as_vec2() / factor, 0.001));
            let screen = resolved.screen_corners.unwrap();
            assert!(screen[0].abs_diff_eq(Vec2::ZERO, 0.001));
            assert!(screen[2].abs_diff_eq(extent.as_vec2(), 0.001));
        }
    }
}

#[test]
fn world_canvas_composes_full_affine_and_offsets() {
    let mut w = world();
    let root = rect(
        &mut w,
        None,
        RectTransform {
            size_delta: Vec2::new(400., 200.),
            anchored_position: Vec2::new(7., 8.),
            ..Default::default()
        },
    );
    let trs = LocalTransform {
        translation: Vec3::new(7., 8., 9.),
        rotation: Quat::from_rotation_z(0.5),
        scale: Vec3::new(2., 3., 1.),
    };
    w.set_local_transform(root, trs).unwrap();
    w.add_component(
        root,
        ComponentDesc::Canvas(Canvas {
            mode: CanvasMode::WorldSpace,
            ..Default::default()
        }),
    )
    .unwrap();
    let r = RectTransform {
        anchor_min: Vec2::ZERO,
        anchor_max: Vec2::ONE,
        ..Default::default()
    }
    .with_offsets(Vec2::new(10., 20.), Vec2::new(-30., -40.))
    .unwrap();
    let child = rect(&mut w, Some(root), r);
    w.set_local_transform(
        child,
        LocalTransform {
            rotation: Quat::from_rotation_z(0.3),
            scale: Vec3::new(3., 0.5, 1.),
            ..Default::default()
        },
    )
    .unwrap();
    let out = w.ui_layout(viewport()).unwrap();
    let resolved = out.rect(child).unwrap();
    assert_eq!(resolved.size, Vec2::new(360., 140.));
    let mut child_trs = w.local_transform(child).unwrap();
    child_trs.translation = Vec3::new(-10., -10., 0.);
    assert!(
        resolved
            .local_to_world
            .abs_diff_eq(trs.matrix() * child_trs.matrix(), 0.001)
    );
    assert!(resolved.screen_corners.is_none());
    w.set_active(root, false).unwrap();
    assert!(w.ui_layout(viewport()).unwrap().rects.is_empty());
}

#[test]
fn typed_atlas_pixels_fonts_and_invalid_rect_errors() {
    let mut fonts = FontBook::new(UiLimits {
        max_atlas_side: 32,
        max_atlas_pixels: 1,
        ..Default::default()
    })
    .unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    assert_eq!(
        fonts.layout(
            &TextRun {
                text: "A".into(),
                ..TextRun::new(id)
            },
            Vec2::splat(100.)
        ),
        Err(UiError::AtlasLimit)
    );
    let mut fonts = FontBook::new(UiLimits {
        max_fonts: 1,
        ..Default::default()
    })
    .unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    assert_eq!(fonts.register(ui_font::font()), Err(UiError::FontLimit));
    let mut second = FontBook::new(UiLimits::default()).unwrap();
    let different = second.register(ui_font::font()).unwrap();
    assert_ne!(id, different);
    assert_eq!(
        second.layout(&TextRun::new(id), Vec2::ONE),
        Err(UiError::MissingFont(id))
    );
    assert_eq!(
        second.layout(&TextRun::new(different), Vec2::new(f32::NAN, 10.)),
        Err(UiError::InvalidRect)
    );
    let mut w = world();
    let invalid = w.create_object(ObjectDesc::default()).unwrap();
    assert!(
        w.add_component(
            invalid,
            ComponentDesc::RectTransform(RectTransform {
                pivot: Vec2::splat(2.),
                ..Default::default()
            })
        )
        .is_err()
    );
}

mod ui_font;

use glam::{Quat, UVec2, Vec2, Vec3};
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, ParentSpace, World, WorldSettings,
};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}
fn rect(
    w: &mut World,
    parent: Option<worldrunner_world_simulation::ObjectHandle>,
    r: RectTransform,
) -> worldrunner_world_simulation::ObjectHandle {
    let o = w.create_object(ObjectDesc::default()).unwrap();
    w.add_component(o, ComponentDesc::RectTransform(r)).unwrap();
    w.set_parent(o, parent, ParentSpace::KeepLocal).unwrap();
    o
}
fn viewport() -> UVec2 {
    UVec2::new(800, 600)
}

#[test]
fn anchors_pivot_nested_scale_and_resize() {
    let mut w = world();
    let root = rect(&mut w, None, RectTransform::default());
    w.add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let child = rect(
        &mut w,
        Some(root),
        RectTransform {
            anchor_min: Vec2::ZERO,
            anchor_max: Vec2::ONE,
            pivot: Vec2::ZERO,
            size_delta: Vec2::new(-40., -60.),
            anchored_position: Vec2::new(10., 20.),
        },
    );
    w.set_local_transform(
        child,
        LocalTransform {
            scale: Vec3::new(2., 3., 1.),
            rotation: Quat::from_rotation_z(0.3),
            ..Default::default()
        },
    )
    .unwrap();
    let leaf = rect(
        &mut w,
        Some(child),
        RectTransform {
            size_delta: Vec2::new(20., 10.),
            ..Default::default()
        },
    );
    let out = w.ui_layout(viewport()).unwrap();
    let c = out.rect(child).unwrap();
    assert_eq!(c.size, Vec2::new(760., 540.));
    assert!(c.corners[0].abs_diff_eq(Vec3::new(10., 20., 0.), 0.001));
    let l = out.rect(leaf).unwrap();
    let expected = c.local_to_world * w.local_transform(leaf).unwrap().matrix();
    assert!(l.local_to_world.x_axis.abs_diff_eq(expected.x_axis, 0.001));
    assert_eq!(
        w.ui_layout(UVec2::new(1000, 700))
            .unwrap()
            .rect(child)
            .unwrap()
            .size,
        Vec2::new(960., 640.)
    );
}

#[test]
fn screen_canvas_owns_effective_zero_scale_but_world_canvas_does_not() {
    let mut w = world();
    let camera = w
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(2., 3., 4.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    for mode in [
        CanvasMode::ScreenSpaceOverlay,
        CanvasMode::ScreenSpaceCamera {
            camera,
            orthographic_size: 3.,
            plane_distance: 10.,
        },
        CanvasMode::WorldSpace,
    ] {
        let root = rect(&mut w, None, RectTransform::default());
        w.set_local_transform(
            root,
            LocalTransform {
                scale: Vec3::ZERO,
                ..Default::default()
            },
        )
        .unwrap();
        w.add_component(
            root,
            ComponentDesc::Canvas(Canvas {
                mode,
                ..Default::default()
            }),
        )
        .unwrap();
        let child = rect(&mut w, Some(root), RectTransform::default());
        let out = w.ui_layout(viewport()).unwrap();
        let r = out.rect(root).unwrap();
        if mode == CanvasMode::WorldSpace {
            assert_eq!(r.local_to_world.determinant().to_bits(), 0_f32.to_bits());
            assert!(
                out.rect(child)
                    .unwrap()
                    .corners
                    .iter()
                    .all(|p| *p == Vec3::ZERO)
            );
        } else {
            assert_ne!(r.local_to_world.determinant().to_bits(), 0_f32.to_bits());
            let screen = r.screen_corners.unwrap();
            assert!(screen[0].abs_diff_eq(Vec2::ZERO, 0.001));
            assert!(screen[2].abs_diff_eq(Vec2::new(800., 600.), 0.001));
        }
        assert_eq!(w.local_transform(root).unwrap().scale, Vec3::ZERO);
    }
}

#[test]
fn disabled_ancestors_and_canvas_suppress_output() {
    let mut w = world();
    let root = rect(&mut w, None, RectTransform::default());
    let canvas = w
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    rect(&mut w, Some(root), RectTransform::default());
    w.set_active(root, false).unwrap();
    assert!(w.ui_layout(viewport()).unwrap().rects.is_empty());
    w.set_active(root, true).unwrap();
    w.set_component(
        canvas,
        ComponentDesc::Canvas(Canvas {
            enabled: false,
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(w.ui_layout(viewport()).unwrap().rects.is_empty());
}

#[test]
fn fonts_text_alignment_wrapping_empty_and_deterministic_metrics() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    let run = TextRun {
        font: id,
        text: "AAA".into(),
        font_size: 20.,
        horizontal: HorizontalAlign::Center,
        vertical: VerticalAlign::Middle,
        ..TextRun::new(id)
    };
    let a = fonts.layout(&run, Vec2::new(25., 100.)).unwrap();
    assert_eq!(a.lines.len(), 2);
    assert_eq!(a.lines[0].width.to_bits(), 24_f32.to_bits());
    assert_eq!(a.glyphs.len(), 3);
    assert_eq!(a, fonts.layout(&run, Vec2::new(25., 100.)).unwrap());
    assert_eq!(a.glyphs[0].origin.x.to_bits(), 0.5_f32.to_bits());
    let empty = fonts.layout(&TextRun::new(id), Vec2::splat(100.)).unwrap();
    assert!(empty.glyphs.is_empty());
    assert!(empty.atlas.alpha.is_empty());
    let right = fonts
        .layout(
            &TextRun {
                text: "A".into(),
                horizontal: HorizontalAlign::Right,
                vertical: VerticalAlign::Bottom,
                ..TextRun::new(id)
            },
            Vec2::splat(100.),
        )
        .unwrap();
    assert!(right.glyphs[0].rect_min.x > 80.);
    assert!(right.glyphs[0].rect_min.y < 20.);
}

#[test]
fn typed_font_shaping_and_resource_errors() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    assert_eq!(fonts.register(vec![0, 1, 2]), Err(UiError::InvalidFont));
    let id = fonts.register(ui_font::font()).unwrap();
    let other = FontBook::new(UiLimits::default()).unwrap();
    assert_eq!(
        other.layout(&TextRun::new(id), Vec2::ONE),
        Err(UiError::MissingFont(id))
    );
    for text in ["a\u{301}", "\u{627}", "\t", "\u{200d}"] {
        assert!(matches!(
            fonts.layout(
                &TextRun {
                    text: text.into(),
                    ..TextRun::new(id)
                },
                Vec2::splat(100.)
            ),
            Err(UiError::UnsupportedShaping(_))
        ));
    }
    let mut small = FontBook::new(UiLimits {
        max_glyphs: 1,
        ..Default::default()
    })
    .unwrap();
    let id = small.register(ui_font::font()).unwrap();
    assert_eq!(
        small.layout(
            &TextRun {
                text: "AA".into(),
                ..TextRun::new(id)
            },
            Vec2::splat(100.)
        ),
        Err(UiError::GlyphLimit)
    );
    let mut tiny = FontBook::new(UiLimits {
        max_atlas_side: 2,
        ..Default::default()
    })
    .unwrap();
    let id = tiny.register(ui_font::font()).unwrap();
    assert_eq!(
        tiny.layout(
            &TextRun {
                text: "A".into(),
                ..TextRun::new(id)
            },
            Vec2::splat(100.)
        ),
        Err(UiError::AtlasLimit)
    );
}
