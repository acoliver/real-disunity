use super::*;
use std::sync::Arc;
use worldrunner_world_simulation::ui::presentation::UiDrawKind;

pub(super) fn fixture() -> (FontBook, TextRun) {
    let mut fonts = fonts();
    let id = fonts.register(ui_font::font()).unwrap();
    (
        fonts,
        TextRun {
            text: "AA".into(),
            font_size: 20.,
            ..TextRun::new(id)
        },
    )
}

#[test]
fn shadows_precede_all_fill_and_keep_final_axis_offset() {
    let (fonts, mut run) = fixture();
    run.color = [1., 0.5, 0.25, 0.5];
    let mut l = list(&fonts);
    l.set_color([0.5, 0.5, 1., 0.5]).unwrap();
    l.set_alpha(0.5).unwrap();
    l.solid_rect(UiRect::new(0., 0., 1., 1.), [1.; 4]).unwrap();
    l.text(
        &run,
        UiTextPlacement {
            rotation_radians: std::f32::consts::FRAC_PI_2,
            ..UiTextPlacement::new(UiRect::new(10., 20., 60., 40.))
        },
        UiTextEffect::Shadow {
            color: [0., 1., 0., 0.5],
            offset: Vec2::new(3., 4.),
        },
    )
    .unwrap();
    l.image(
        images::image(),
        UiRect::new(0., 0., 1., 1.),
        UiImageOptions::default(),
    )
    .unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 6);
    assert_eq!(p.coverage.len(), 1);
    assert_eq!(
        (p.draws[1].color).map(f32::to_bits),
        ([0., 0.5, 0., 0.0625]).map(f32::to_bits)
    );
    assert_eq!(
        (p.draws[3].color).map(f32::to_bits),
        ([0.5, 0.25, 0.25, 0.125]).map(f32::to_bits)
    );
    for i in 1..=2 {
        for (s, f) in p.draws[i].corners.iter().zip(p.draws[i + 2].corners) {
            assert!((s[0] - f[0] - 3.).abs() < 0.00001);
            assert!((s[1] - f[1] + 4.).abs() < 0.00001);
        }
        assert_eq!(
            (p.draws[i].uv_rect).map(f32::to_bits),
            (p.draws[i + 2].uv_rect).map(f32::to_bits)
        );
    }
    assert_eq!(
        p.draws.iter().map(|d| d.kind).collect::<Vec<_>>(),
        [
            UiDrawKind::Graphic,
            UiDrawKind::Text,
            UiDrawKind::Text,
            UiDrawKind::Text,
            UiDrawKind::Text,
            UiDrawKind::Graphic
        ]
    );
    for (i, d) in p.draws.iter().enumerate() {
        assert_eq!(d.hierarchy_order, 5 + i);
    }
}

#[test]
fn nonuniform_text_placement_maps_shared_layout_without_clipping_to_box() {
    let (fonts, run) = fixture();
    let layout_size = Vec2::new(30., 20.);
    let layout = fonts.layout(&run, layout_size).unwrap();
    let mut l = list(&fonts);
    l.text(
        &run,
        UiTextPlacement {
            layout_size,
            ..UiTextPlacement::new(UiRect::new(10., 20., 60., 10.))
        },
        UiTextEffect::None,
    )
    .unwrap();
    let p = l.finish().unwrap();
    let g = &layout.glyphs[0];
    assert_eq!(
        (p.draws[0].corners[0]).map(f32::to_bits),
        ([
            10. + g.rect_min.x * 2.,
            80. - (20. + (20. - g.rect_min.y) * 0.5),
            0.
        ])
        .map(f32::to_bits)
    );
    assert_eq!(p.draws[0].clip_rect, [0, 0, 100, 80]);
}

#[test]
fn coverage_is_shared_deduplicated_and_survives_font_book() {
    let (fonts, run) = fixture();
    let layout = fonts.layout(&run, Vec2::new(60., 40.)).unwrap();
    let mut l = list(&fonts);
    for _ in 0..2 {
        l.text(
            &run,
            UiTextPlacement::new(UiRect::new(0., 0., 60., 40.)),
            UiTextEffect::None,
        )
        .unwrap();
    }
    let p = l.finish().unwrap();
    assert_eq!(p.coverage.len(), 1);
    assert!(Arc::ptr_eq(&layout.atlas.alpha, &p.coverage[&0].bytes));
    drop(fonts);
    assert!(!p.coverage[&0].bytes.is_empty());
}

#[test]
fn zero_area_and_empty_clip_text_do_not_layout_or_reference_resources() {
    let (fonts, run) = fixture();
    let mut l = list(&fonts);
    l.text(
        &run,
        UiTextPlacement {
            layout_size: Vec2::ZERO,
            ..UiTextPlacement::new(UiRect::new(0., 0., 0., 10.))
        },
        UiTextEffect::None,
    )
    .unwrap();
    l.push_clip(UiRect::new(0., 0., 0., 0.)).unwrap();
    l.text(
        &run,
        UiTextPlacement::new(UiRect::new(0., 0., 60., 40.)),
        UiTextEffect::None,
    )
    .unwrap();
    l.pop_clip().unwrap();
    let p = l.finish().unwrap();
    assert!(p.draws.is_empty() && p.coverage.is_empty());
    assert_eq!(fonts.cache_stats().layouts, 0);
}

#[test]
fn per_list_characters_pixels_and_vertices_are_atomic() {
    let (fonts, run) = fixture();
    let placement = UiTextPlacement::new(UiRect::new(0., 0., 60., 40.));
    let layout = fonts.layout(&run, placement.layout_size).unwrap();
    let pixels = layout.atlas.alpha.len();
    let mut l = UiDrawList::new(
        UVec2::new(100, 80),
        &fonts,
        UiOrder::default(),
        UiDrawLimits {
            max_characters: 2,
            max_atlas_pixels: pixels,
            max_vertices: 12,
            ..UiDrawLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        l.text(
            &run,
            placement,
            UiTextEffect::Shadow {
                color: [1.; 4],
                offset: Vec2::ONE
            }
        ),
        Err(UiError::DrawLimit)
    );
    assert_eq!(l.draw_range(), 0..0);
    l.text(&run, placement, UiTextEffect::None).unwrap();
    assert_eq!(
        l.text(&run, placement, UiTextEffect::None),
        Err(UiError::GlyphLimit)
    );
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 2);
    assert_eq!(p.coverage.len(), 1);
    let mut l = UiDrawList::new(
        UVec2::new(100, 80),
        &fonts,
        UiOrder::default(),
        UiDrawLimits {
            max_atlas_pixels: pixels - 1,
            ..UiDrawLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        l.text(&run, placement, UiTextEffect::None),
        Err(UiError::AtlasLimit)
    );
    assert!(l.finish().unwrap().coverage.is_empty());
}

#[test]
fn list_measurement_explicitly_controls_wrapping() {
    let (fonts, mut run) = fixture();
    run.text = "AA AA".into();
    run.horizontal_overflow = HorizontalOverflow::Overflow;
    let l = list(&fonts);
    assert_eq!(
        f32::to_bits(l.measure_text(&run, None).unwrap().width),
        f32::to_bits(60.)
    );
    let m = l.measure_text(&run, Some(30.)).unwrap();
    assert_eq!(f32::to_bits(m.width), f32::to_bits(24.));
    assert_eq!(m.lines.len(), 2);
}
