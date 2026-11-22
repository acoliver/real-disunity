//! Baseline coverage and distance layout contracts, using only a synthetic font.
#![cfg(feature = "ui")]
mod ui_font;
use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::*;

fn fixture() -> (FontBook, TextRun) {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let mut run = TextRun::new(fonts.register(ui_font::font()).unwrap());
    run.text = "AA".into();
    run.font_size = 20.;
    (fonts, run)
}

#[test]
fn font_book_produces_padded_distance_layout() {
    let (fonts, run) = fixture();
    let result = fonts.layout_with_raster(
        &run,
        Vec2::new(50., 40.),
        GlyphRaster {
            mode: RenderMode::Sdf,
            padding: 5,
        },
    );
    assert!(result.is_ok(), "padded distance layout failed: {result:?}");
}

#[test]
fn distance_effects_emit_exact_padded_quads_uvs_parameters_and_order() {
    use worldrunner_world_simulation::ui::presentation::{
        UiDistanceMode, UiDistanceStyle, UiDrawKind,
    };
    let (fonts, run) = fixture();
    let mut list = UiDrawList::new(
        UVec2::splat(100),
        &fonts,
        UiOrder {
            layer: 2,
            order: 3,
            first_hierarchy: 7,
        },
        UiDrawLimits::default(),
    )
    .unwrap();
    for effect in [
        UiTextEffect::Outline {
            color: [1., 0., 0., 0.5],
            size: 2.,
        },
        UiTextEffect::Glow {
            color: [0., 1., 0., 0.5],
            size: 2.,
        },
    ] {
        list.text(
            &run,
            UiTextPlacement::new(UiRect::new(20., 20., 50., 40.)),
            effect,
        )
        .unwrap();
    }
    let presentation = list.finish().unwrap();
    assert_eq!(presentation.draws.len(), 4);
    assert_eq!(presentation.coverage.len(), 1);
    assert_eq!(
        (
            presentation.coverage[&0].width,
            presentation.coverage[&0].height
        ),
        (2048, 26)
    );
    for (i, draw) in presentation.draws.iter().enumerate() {
        let x = if i % 2 == 0 { 15. } else { 27. };
        assert_eq!(
            draw.corners,
            [
                [x, 59., 0.],
                [x + 20., 59., 0.],
                [x + 20., 83., 0.],
                [x, 83., 0.]
            ]
        );
        let u = if i % 2 == 0 { 1. } else { 23. };
        assert_eq!(
            draw.uv_rect.map(f32::to_bits),
            [u / 2048., 1. / 26., (u + 20.) / 2048., 25. / 26.].map(f32::to_bits)
        );
        assert_eq!(draw.color.map(f32::to_bits), [1.; 4].map(f32::to_bits));
        assert_eq!(
            draw.distance_field,
            Some(UiDistanceStyle {
                spread_px: 5.,
                radius_px: 2.,
                mode: if i < 2 {
                    UiDistanceMode::Outline
                } else {
                    UiDistanceMode::Glow
                },
                effect_color: if i < 2 {
                    [1., 0., 0., 0.5]
                } else {
                    [0., 1., 0., 0.5]
                },
            })
        );
        assert_eq!(
            (draw.sorting_layer, draw.sorting_order, draw.hierarchy_order),
            (2, 3, 7 + i)
        );
        assert_eq!(draw.kind, UiDrawKind::Text);
        assert_eq!(draw.texture, Some(UiTexture::Coverage(0)));
        assert_eq!(draw.clip_rect, [0, 0, 100, 100]);
        assert!(draw.corner_colors.is_none() && draw.triangle_uv.is_none());
        assert!(!draw.depth_test);
    }
}

#[test]
fn coverage_and_shadow_match_baseline_bytes_and_geometry() {
    let (fonts, run) = fixture();
    for effect in [
        UiTextEffect::None,
        UiTextEffect::Shadow {
            color: [0.25, 0.5, 1., 0.5],
            offset: Vec2::new(3., 4.),
        },
    ] {
        let mut list = UiDrawList::new(
            UVec2::splat(100),
            &fonts,
            UiOrder::default(),
            UiDrawLimits::default(),
        )
        .unwrap();
        list.text(
            &run,
            UiTextPlacement::new(UiRect::new(20., 20., 50., 40.)),
            effect,
        )
        .unwrap();
        let presentation = list.finish().unwrap();
        let texture = &presentation.coverage[&0];
        assert_eq!((texture.width, texture.height), (2048, 14));
        let glyph = [
            [91, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [240, 33, 0, 0, 0, 0, 0, 0, 0, 0],
            [255, 197, 4, 0, 0, 0, 0, 0, 0, 0],
            [255, 255, 128, 0, 0, 0, 0, 0, 0, 0],
            [255, 255, 251, 58, 0, 0, 0, 0, 0, 0],
            [255, 255, 255, 222, 15, 0, 0, 0, 0, 0],
            [255, 255, 255, 255, 164, 0, 0, 0, 0, 0],
            [255, 255, 255, 255, 255, 91, 0, 0, 0, 0],
            [255, 255, 255, 255, 255, 240, 33, 0, 0, 0],
            [255, 255, 255, 255, 255, 255, 197, 4, 0, 0],
            [255, 255, 255, 255, 255, 255, 255, 128, 0, 0],
            [255, 255, 255, 255, 255, 255, 255, 251, 58, 0],
            [255, 255, 255, 255, 255, 255, 255, 255, 222, 15],
            [255, 255, 255, 255, 255, 255, 255, 255, 255, 164],
        ];
        for (row, expected) in texture.bytes.chunks_exact(2048).zip(glyph) {
            assert_eq!(&row[..10], &expected);
            assert_eq!(&row[10..20], &expected);
            assert!(row[20..].iter().all(|&v| v == 0));
        }
        assert_eq!(presentation.coverage.len(), 1);
        let passes = if effect == UiTextEffect::None { 1 } else { 2 };
        assert_eq!(presentation.draws.len(), passes * 2);
        for (i, draw) in presentation.draws.iter().enumerate() {
            let shadow = passes == 2 && i < 2;
            let x = 20. + if i % 2 == 0 { 0. } else { 12. } + if shadow { 3. } else { 0. };
            let y = 64. - if shadow { 4. } else { 0. };
            assert_eq!(
                draw.corners,
                [
                    [x, y, 0.],
                    [x + 10., y, 0.],
                    [x + 10., y + 14., 0.],
                    [x, y + 14., 0.]
                ]
            );
            let u = if i % 2 == 0 { 0. } else { 10. / 2048. };
            assert_eq!(
                draw.uv_rect.map(f32::to_bits),
                [u, 0., u + 10. / 2048., 1.].map(f32::to_bits)
            );
            assert_eq!(
                draw.color.map(f32::to_bits),
                if shadow {
                    [0.25, 0.5, 1., 0.5]
                } else {
                    [1.; 4]
                }
                .map(f32::to_bits)
            );
            assert_eq!(draw.hierarchy_order, i);
            assert_eq!(draw.clip_rect, [0, 0, 100, 100]);
            assert_eq!(draw.texture, Some(UiTexture::Coverage(0)));
            assert!(draw.distance_field.is_none());
        }
    }
}
