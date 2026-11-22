//! Synthetic font distance raster and immediate effect contracts. CPU only.
#![cfg(feature = "ui")]
mod ui_font;
use glam::{UVec2, Vec2};
use std::sync::Arc;
use worldrunner_world_simulation::ui::presentation::UiDistanceMode;
use worldrunner_world_simulation::ui::*;

fn fixture(limits: UiLimits) -> (FontBook, TextRun) {
    let mut fonts = FontBook::new(limits).unwrap();
    let mut run = TextRun::new(fonts.register(ui_font::font()).unwrap());
    run.text = "AA".into();
    run.font_size = 20.;
    (fonts, run)
}
fn list(fonts: &FontBook, limits: UiDrawLimits) -> UiDrawList<'_> {
    UiDrawList::new(UVec2::splat(100), fonts, UiOrder::default(), limits).unwrap()
}
fn placement() -> UiTextPlacement {
    UiTextPlacement::new(UiRect::new(20., 20., 50., 40.))
}

#[test]
fn padded_sdf_uses_metrics_cache_and_bounded_shared_atlas() {
    let (fonts, run) = fixture(UiLimits::default());
    let raster = GlyphRaster {
        mode: RenderMode::Sdf,
        padding: 5,
    };
    let plain = fonts.layout(&run, Vec2::new(50., 40.)).unwrap();
    let sdf = fonts
        .layout_with_raster(&run, Vec2::new(50., 40.), raster)
        .unwrap();
    assert_eq!(plain.lines, sdf.lines);
    assert_eq!(plain.content_height.to_bits(), sdf.content_height.to_bits());
    assert_eq!(
        sdf.glyphs[0].rect_min,
        plain.glyphs[0].rect_min - Vec2::splat(5.)
    );
    assert_eq!(
        sdf.glyphs[0].rect_max,
        plain.glyphs[0].rect_max + Vec2::splat(5.)
    );
    assert!(sdf.atlas.alpha.iter().any(|&v| v > 128));
    assert!(sdf.atlas.alpha.iter().any(|&v| v > 0 && v < 128));
    assert!(Arc::ptr_eq(
        &sdf,
        &fonts
            .layout_with_raster(&run, Vec2::new(50., 40.), raster)
            .unwrap()
    ));
    assert!(!Arc::ptr_eq(&plain.atlas.alpha, &sdf.atlas.alpha));
    assert_eq!(fonts.layout(&run, Vec2::new(50., 40.)).unwrap(), plain);
}

#[test]
fn effects_share_fill_atlas_preserve_order_and_alpha() {
    let (fonts, mut run) = fixture(UiLimits::default());
    run.color = [0.25, 0.5, 1., 0.5];
    run.spans.push(TextSpan {
        range: 0..1,
        color: [0.5; 4],
        link: None,
    });
    let mut l = list(&fonts, UiDrawLimits::default());
    l.set_color([0.5; 4]).unwrap();
    l.set_alpha(0.5).unwrap();
    l.text(
        &run,
        placement(),
        UiTextEffect::Outline {
            color: [1., 0., 0., 0.5],
            size: 2.,
        },
    )
    .unwrap();
    l.text(
        &run,
        placement(),
        UiTextEffect::Glow {
            color: [0., 1., 0., 0.5],
            size: 2.,
        },
    )
    .unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 4);
    assert_eq!(p.coverage.len(), 1);
    for (i, draw) in p.draws.iter().enumerate() {
        assert_eq!(draw.hierarchy_order, i);
        assert_eq!(draw.texture, Some(UiTexture::Coverage(0)));
        let style = draw.distance_field.unwrap();
        assert_eq!(style.spread_px.to_bits(), 5_f32.to_bits());
        assert_eq!(style.radius_px.to_bits(), 2_f32.to_bits());
        assert_eq!(
            style.mode,
            if i < 2 {
                UiDistanceMode::Outline
            } else {
                UiDistanceMode::Glow
            }
        );
        assert_eq!(
            style.effect_color[3].to_bits(),
            (if i % 2 == 0 { 0.03125_f32 } else { 0.0625 }).to_bits()
        );
    }
    assert_eq!(
        p.draws[0].color.map(f32::to_bits),
        [0.0625, 0.125, 0.25, 0.0625].map(f32::to_bits)
    );
}

#[test]
fn effect_errors_are_atomic_and_zero_radius_is_coverage() {
    let (fonts, run) = fixture(UiLimits::default());
    let mut l = list(&fonts, UiDrawLimits::default());
    for size in [-1., f32::NAN, f32::INFINITY] {
        assert_eq!(
            l.text(
                &run,
                placement(),
                UiTextEffect::Glow {
                    color: [1.; 4],
                    size
                }
            ),
            Err(UiError::InvalidTextEffect)
        );
    }
    assert_eq!(
        l.text(
            &run,
            placement(),
            UiTextEffect::Outline {
                color: [1.; 4],
                size: 256.
            }
        ),
        Err(UiError::DistanceFieldLimit)
    );
    assert_eq!(l.draw_range(), 0..0);
    l.text(
        &run,
        placement(),
        UiTextEffect::Glow {
            color: [1.; 4],
            size: 0.,
        },
    )
    .unwrap();
    let p = l.finish().unwrap();
    assert!(p.draws.iter().all(|d| d.distance_field.is_none()));
    assert_eq!(p.coverage.len(), 1);
}

#[test]
fn effect_vertices_characters_pixels_and_minification_are_bounded() {
    let (fonts, run) = fixture(UiLimits::default());
    let effect = UiTextEffect::Glow {
        color: [1.; 4],
        size: 8.,
    };
    for (limits, error) in [
        (
            UiDrawLimits {
                max_vertices: 11,
                ..UiDrawLimits::default()
            },
            UiError::DrawLimit,
        ),
        (
            UiDrawLimits {
                max_characters: 1,
                ..UiDrawLimits::default()
            },
            UiError::GlyphLimit,
        ),
        (
            UiDrawLimits {
                max_atlas_pixels: 1,
                ..UiDrawLimits::default()
            },
            UiError::AtlasLimit,
        ),
    ] {
        let mut l = list(&fonts, limits);
        assert_eq!(l.text(&run, placement(), effect), Err(error));
        assert!(l.finish().unwrap().coverage.is_empty());
    }
    let mut p = placement();
    p.destination.size *= 0.001;
    let mut l = list(&fonts, UiDrawLimits::default());
    assert_eq!(l.text(&run, p, effect), Err(UiError::DistanceFieldLimit));
    p.destination.size = Vec2::ZERO;
    l.text(&run, p, effect).unwrap();
    assert!(l.finish().unwrap().draws.is_empty());
}

#[test]
fn rotation_and_nonuniform_scale_use_minimum_scale_padding() {
    let (fonts, run) = fixture(UiLimits::default());
    let mut p = placement();
    p.destination.size.x *= 2.;
    p.rotation_radians = std::f32::consts::FRAC_PI_4;
    let mut l = list(&fonts, UiDrawLimits::default());
    l.text(
        &run,
        p,
        UiTextEffect::Outline {
            color: [1.; 4],
            size: 2.,
        },
    )
    .unwrap();
    let p = l.finish().unwrap();
    assert_eq!(
        p.draws[0].distance_field.unwrap().spread_px.to_bits(),
        5_f32.to_bits()
    );
    assert!(p.draws[0].corners.iter().flatten().all(|v| v.is_finite()));
}

#[test]
fn shared_face_and_layout_sdf_have_identical_bytes_with_opposite_row_origins() {
    let (fonts, mut run) = fixture(UiLimits::default());
    run.text = "A".into();
    let layout = fonts
        .layout_with_raster(
            &run,
            Vec2::new(50., 40.),
            GlyphRaster {
                mode: RenderMode::Sdf,
                padding: 5,
            },
        )
        .unwrap();
    let glyph = &layout.glyphs[0];
    let size = glyph.atlas_size;
    let mut faces = FaceBook::default();
    let face = faces.load(ui_font::font(), 0, 20.).unwrap();
    let mut bytes = vec![0; usize::try_from(size.x * size.y).unwrap()];
    faces
        .render_glyph(
            face,
            GlyphJob {
                glyph: 1,
                mode: RenderMode::Sdf,
                padding: 5,
                rect: GlyphRect {
                    x: 0,
                    y: 0,
                    width: size.x,
                    height: size.y,
                },
            },
            &mut bytes,
            size.x,
            size.y,
        )
        .unwrap();
    let stride = layout.atlas.size.x;
    for y in 0..size.y {
        let start = usize::try_from((glyph.atlas_min.y + y) * stride + glyph.atlas_min.x).unwrap();
        let face_start = usize::try_from((size.y - 1 - y) * size.x).unwrap();
        assert_eq!(
            &layout.atlas.alpha[start..start + usize::try_from(size.x).unwrap()],
            &bytes[face_start..face_start + usize::try_from(size.x).unwrap()]
        );
    }
    for y in glyph.atlas_min.y - 1..=glyph.atlas_min.y + size.y {
        for x in [glyph.atlas_min.x - 1, glyph.atlas_min.x + size.x] {
            assert_eq!(
                layout.atlas.alpha[usize::try_from(y * stride + x).unwrap()],
                0
            );
        }
    }
    for x in glyph.atlas_min.x - 1..=glyph.atlas_min.x + size.x {
        for y in [glyph.atlas_min.y - 1, glyph.atlas_min.y + size.y] {
            assert_eq!(
                layout.atlas.alpha[usize::try_from(y * stride + x).unwrap()],
                0
            );
        }
    }
}

#[test]
fn sdf_side_and_scratch_limits_fail_without_cache_publication() {
    let (fonts, mut run) = fixture(UiLimits::default());
    run.text = "A".into();
    let size = Vec2::new(50., 40.);
    let at_limit = fonts
        .layout_with_raster(
            &run,
            size,
            GlyphRaster {
                mode: RenderMode::Sdf,
                padding: 121,
            },
        )
        .unwrap();
    assert_eq!(at_limit.glyphs[0].atlas_size.y, 256);
    let stats = fonts.cache_stats();
    for padding in [0, 122, u32::MAX] {
        assert_eq!(
            fonts.layout_with_raster(
                &run,
                size,
                GlyphRaster {
                    mode: RenderMode::Sdf,
                    padding
                }
            ),
            Err(UiError::DistanceFieldLimit)
        );
        assert_eq!(fonts.cache_stats(), stats);
    }
    let (fonts, run) = fixture(UiLimits {
        max_atlas_pixels: 479,
        ..UiLimits::default()
    });
    assert_eq!(
        fonts.layout_with_raster(
            &run,
            size,
            GlyphRaster {
                mode: RenderMode::Sdf,
                padding: 5
            }
        ),
        Err(UiError::DistanceFieldLimit)
    );
    assert_eq!(fonts.cache_stats().layouts, 0);
}

#[test]
fn distance_style_requires_finite_affine_text_and_sufficient_spread() {
    use worldrunner_world_simulation::ui::presentation::{
        UiDistanceStyle, validate_distance_mapping,
    };
    let corners = [[0., 0., 0.], [16., 0., 0.], [16., 16., 0.], [0., 16., 0.]];
    let uv = [0., 0., 1., 1.];
    let mut style = UiDistanceStyle {
        spread_px: 8.,
        radius_px: 7.5,
        mode: UiDistanceMode::Outline,
        effect_color: [1.; 4],
    };
    assert_eq!(style.validate_extent(corners, uv, UVec2::splat(16)), Ok(()));
    style.radius_px = 7.501;
    assert_eq!(
        style.validate_extent(corners, uv, UVec2::splat(16)),
        Err(UiError::DistanceFieldLimit)
    );
    assert_eq!(
        validate_distance_mapping(corners, [0.; 4]),
        Err(UiError::InvalidDistanceMapping)
    );
    let mut non_affine = corners;
    non_affine[2][0] += 1.;
    assert_eq!(
        validate_distance_mapping(non_affine, uv),
        Err(UiError::InvalidDistanceMapping)
    );
}

#[test]
fn distance_rotation_accepts_pixel_roundoff_near_viewport_limit() {
    let (fonts, run) = fixture(UiLimits::default());
    for (angle, y) in [0.1, 0.2, 0.5, 0.7, 1.1]
        .into_iter()
        .flat_map(|a| [16.007, 16000.007].map(|y| (a, y)))
    {
        let mut l = UiDrawList::new(
            UVec2::splat(16384),
            &fonts,
            UiOrder::default(),
            UiDrawLimits::default(),
        )
        .unwrap();
        let p = UiTextPlacement {
            destination: UiRect::new(16000.003, y, 39., 28.),
            rotation_radians: angle,
            ..placement()
        };
        l.text(
            &run,
            p,
            UiTextEffect::Outline {
                color: [1.; 4],
                size: 2.,
            },
        )
        .unwrap();
    }
}
