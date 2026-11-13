#![cfg(feature = "ui")]
#[path = "ui_font/positioning.rs"]
mod positioning;
mod ui_font;

use glam::Vec2;
use positioning::{Feature, Lookup, Script, default_gpos, font, gpos, kern, pair, words};
use worldrunner_world_simulation::ui::{
    FaceBook, FaceLimits, FontBook, TextRun, UiError, UiLimits,
};

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.00001,
        "actual {actual}, expected {expected}"
    );
}

fn assert_layout(bytes: Vec<u8>, text: &str, width: f32, widths: &[f32], origins: &[f32]) {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(bytes).unwrap();
    let run = TextRun {
        text: text.into(),
        font_size: 20.,
        ..TextRun::new(id)
    };
    let layout = fonts.layout(&run, Vec2::new(width, 200.)).unwrap();
    assert_eq!(layout.lines.len(), widths.len());
    for (line, &expected) in layout.lines.iter().zip(widths) {
        close(line.width, expected);
    }
    assert_eq!(layout.glyphs.len(), origins.len());
    for (glyph, &expected) in layout.glyphs.iter().zip(origins) {
        close(glyph.origin.x, expected);
    }
}

fn adjustment(bytes: &[u8], first: u32, second: u32) -> f32 {
    let mut faces = FaceBook::default();
    let id = faces.load(bytes.to_vec(), 0, 20.).unwrap();
    faces
        .pair_adjustments(id, &[second, first, second])
        .unwrap()
        .iter()
        .find(|p| p.first == first && p.second == second)
        .map_or(0., |p| p.x_advance)
}

fn assert_flow(bytes: Vec<u8>, advance: f32) {
    close(adjustment(&bytes, 1, 2), advance);
    let width = 24. + advance;
    let origin = 12. + advance;
    assert_layout(bytes.clone(), "AV", 100., &[width], &[0., origin]);
    assert_layout(bytes.clone(), "AV", width + 1., &[width], &[0., origin]);
    assert_layout(bytes.clone(), "AV", width - 1., &[12., 12.], &[0., 0.]);
    assert_layout(bytes.clone(), "A\nV", 100., &[12., 12.], &[0., 0.]);
    assert_layout(bytes, "A AV", 35., &[12., width], &[0., 0., origin]);
}

#[test]
fn gpos_pair_formats_and_extensions_share_measurement_wrapping_placement_and_face_pairs() {
    for format in [1, 2] {
        for extension in [false, true] {
            let bytes = font(Some(default_gpos(&[Lookup::pair(format, extension)])), None);
            let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
            assert_eq!(face.glyph_index('A'), Some(ttf_parser::GlyphId(1)));
            assert_eq!(face.glyph_index('V'), Some(ttf_parser::GlyphId(2)));
            assert_eq!(face.units_per_em(), 1000);
            assert!(face.tables().gpos.is_some());
            assert!(face.tables().kern.is_none());
            assert_flow(bytes, -2.);
        }
    }
}

#[test]
fn unadjusted_font_has_width_24_and_origins_0_and_12() {
    assert_flow(font(None, None), 0.);
    assert_layout(ui_font::font(), "AV", 100., &[24.], &[0., 12.]);
}

#[test]
fn ignored_placement_vertical_second_record_and_device_fields_do_not_move_the_pen() {
    for format in [1, 2] {
        for extension in [false, true] {
            let lookup = Lookup::new(vec![pair(format, -100, false, true)], extension);
            assert_flow(font(Some(default_gpos(&[lookup])), None), -2.);
        }
    }
}

#[test]
fn class_zero_applies_only_when_the_first_glyph_is_covered() {
    for extension in [false, true] {
        let lookup = Lookup::new(vec![pair(2, -100, true, false)], extension);
        let bytes = font(Some(default_gpos(&[lookup])), None);
        assert_flow(bytes.clone(), -2.);
        close(adjustment(&bytes, 1, 1), -2.);
        close(adjustment(&bytes, 2, 1), 0.);
        close(adjustment(&bytes, 2, 2), 0.);
        assert_layout(bytes.clone(), "AA", 100., &[22.], &[0., 10.]);
        assert_layout(bytes, "VA", 100., &[24.], &[0., 12.]);
    }
}

#[test]
fn uncovered_and_unmatched_pairs_do_not_adjust() {
    for format in [1, 2] {
        let bytes = font(Some(default_gpos(&[Lookup::pair(format, false)])), None);
        close(adjustment(&bytes, 1, 1), 0.);
        close(adjustment(&bytes, 2, 1), 0.);
        assert_layout(bytes.clone(), "AA", 100., &[24.], &[0., 12.]);
        assert_layout(bytes, "VA", 100., &[24.], &[0., 12.]);
    }
}

#[test]
fn gpos_precedence_never_sums_legacy_or_resurrects_it_on_a_miss() {
    for format in [1, 2] {
        for extension in [false, true] {
            let legacy = kern(&[(1, 1, -50), (1, 2, -50), (2, 1, -50)]);
            let bytes = font(
                Some(default_gpos(&[Lookup::pair(format, extension)])),
                Some(legacy),
            );
            assert_flow(bytes.clone(), -2.);
            close(adjustment(&bytes, 1, 1), 0.);
            close(adjustment(&bytes, 2, 1), 0.);
            assert_layout(bytes.clone(), "AA", 100., &[24.], &[0., 12.]);
            assert_layout(bytes, "VA", 100., &[24.], &[0., 12.]);
        }
    }
}

#[test]
fn legacy_only_horizontal_kerning_is_unchanged() {
    assert_flow(font(None, Some(kern(&[(1, 2, -50)]))), -1.);
    let mut vertical = kern(&[(1, 2, -50)]);
    vertical[8..10].copy_from_slice(&0_u16.to_be_bytes());
    assert_flow(font(None, Some(vertical)), 0.);
}

#[test]
fn distinct_lookups_accumulate_once_and_only_the_first_matching_subtable_applies() {
    let lookups = [
        Lookup::new(
            vec![pair(1, -40, false, false), pair(2, -300, false, false)],
            false,
        ),
        Lookup::new(vec![pair(2, -60, false, false)], true),
    ];
    let features = [Feature::kern(vec![1, 0, 1]), Feature::kern(vec![0])];
    let scripts = [Script {
        tag: *b"DFLT",
        required: Some(0),
        features: vec![1, 0, 1],
    }];
    assert_flow(font(Some(gpos(&scripts, &features, &lookups)), None), -2.);
}

#[test]
fn a_zero_match_stops_subtable_search_but_a_missing_pair_does_not() {
    let zero = Lookup::new(
        vec![pair(1, 0, false, false), pair(2, -100, false, false)],
        false,
    );
    assert_flow(font(Some(default_gpos(&[zero])), None), 0.);
    let mut miss = pair(1, -500, false, false);
    miss[14..16].copy_from_slice(&0_u16.to_be_bytes());
    let lookup = Lookup::new(vec![miss, pair(2, -100, false, false)], false);
    assert_flow(font(Some(default_gpos(&[lookup])), None), -2.);
}

#[test]
fn dflt_wins_over_latn_and_a_required_kern_feature_is_selected() {
    let lookups = [
        Lookup::pair(1, false),
        Lookup::new(vec![pair(2, -200, false, false)], false),
    ];
    let features = [Feature::kern(vec![0]), Feature::kern(vec![1])];
    let scripts = [
        Script {
            tag: *b"DFLT",
            required: Some(0),
            features: vec![],
        },
        Script {
            tag: *b"latn",
            required: None,
            features: vec![1],
        },
    ];
    assert_flow(font(Some(gpos(&scripts, &features, &lookups)), None), -2.);
}

#[test]
fn latn_is_used_when_dflt_has_no_reachable_kern_feature() {
    let features = [
        Feature::kern(vec![0]),
        Feature {
            tag: *b"xxxx",
            lookups: vec![1],
        },
    ];
    let lookups = [
        Lookup::pair(1, false),
        Lookup::new(vec![pair(1, -400, false, false)], false),
    ];
    for dflt in [false, true] {
        let mut scripts = Vec::new();
        if dflt {
            scripts.push(Script {
                tag: *b"DFLT",
                required: None,
                features: vec![1],
            });
        }
        scripts.push(Script::default_kern(*b"latn"));
        assert_flow(font(Some(gpos(&scripts, &features, &lookups)), None), -2.);
    }
}

#[test]
fn a_reachable_empty_kern_feature_prevents_switching_script_branches() {
    let scripts = [
        Script::default_kern(*b"DFLT"),
        Script {
            tag: *b"latn",
            required: None,
            features: vec![1],
        },
    ];
    let features = [Feature::kern(vec![]), Feature::kern(vec![0])];
    assert_flow(
        font(
            Some(gpos(&scripts, &features, &[Lookup::pair(1, false)])),
            None,
        ),
        0.,
    );
}

#[test]
fn an_unreachable_gpos_kern_feature_still_suppresses_legacy() {
    for script in [*b"arab", *b"DFLT"] {
        let scripts = [Script {
            tag: script,
            required: None,
            features: vec![],
        }];
        let gpos = gpos(
            &scripts,
            &[Feature::kern(vec![0])],
            &[Lookup::pair(1, false)],
        );
        assert_flow(font(Some(gpos), Some(kern(&[(1, 2, -50)]))), 0.);
    }
}

#[test]
fn unsupported_gpos_lookups_do_not_fall_back_to_legacy() {
    let single = Lookup {
        kind: 1,
        subtables: vec![words(&[1, 8, 4, 100, 1, 1, 1])],
    };
    assert_flow(
        font(Some(default_gpos(&[single])), Some(kern(&[(1, 2, -50)]))),
        0.,
    );
}

#[test]
fn a_gpos_table_without_a_kern_feature_uses_legacy() {
    let gpos = gpos(
        &[Script::default_kern(*b"DFLT")],
        &[Feature {
            tag: *b"xxxx",
            lookups: vec![0],
        }],
        &[Lookup::pair(1, false)],
    );
    assert_flow(font(Some(gpos), Some(kern(&[(1, 2, -50)]))), -1.);
}

#[test]
fn text_field_caret_uses_the_same_pair_advance_as_its_glyph_layout() {
    use worldrunner_world_simulation::ui::{
        Canvas, RectTransform, TextFieldStyle, UiControl, UiKey, UiRouter, UiTextField,
        contribution,
    };
    use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, World, WorldSettings};
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts
        .register(font(Some(default_gpos(&[Lookup::pair(1, false)])), None))
        .unwrap();
    let run = TextRun {
        text: "AV".into(),
        font_size: 20.,
        ..TextRun::new(id)
    };
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .add_component(object, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let mut router = UiRouter::default();
    router.controls.insert(object, UiControl::text_field());
    router
        .text_fields
        .insert(object, UiTextField::new(run, 10, &fonts).unwrap());
    let layout = world.ui_layout([100, 100].into()).unwrap();
    router
        .key(&world, &layout, UiKey::Tab, true, false)
        .unwrap();
    let left = layout.rect(object).unwrap().screen_corners.unwrap()[0].x;
    let mut plan = contribution::extract_ui_layout(&world, &fonts, layout).unwrap();
    router
        .text_fields_plan(
            &world,
            &fonts,
            &mut plan,
            TextFieldStyle {
                caret: [1.; 4],
                selection: [1.; 4],
            },
        )
        .unwrap();
    let caret = &plan.graphics.last().unwrap().graphic.quads[0];
    close(caret.corners[0][0] - left, 22.);
}

#[test]
fn positioning_storage_is_charged_to_font_and_face_budgets_and_released_on_unload() {
    let bytes = font(Some(default_gpos(&[Lookup::pair(2, false)])), None);
    let storage = std::mem::size_of::<(u32, u16)>();
    let mut fonts = FontBook::new(UiLimits {
        max_font_bytes: bytes.len(),
        ..UiLimits::default()
    })
    .unwrap();
    assert_eq!(fonts.register(bytes.clone()), Err(UiError::FontLimit));
    let mut fonts = FontBook::new(UiLimits {
        max_font_bytes: bytes.len() + storage,
        ..UiLimits::default()
    })
    .unwrap();
    fonts.register(bytes.clone()).unwrap();
    let limits = FaceLimits {
        max_face_bytes: bytes.len() + storage,
        max_total_bytes: bytes.len() + storage,
        max_faces: 2,
    };
    let mut faces = FaceBook::new(FaceLimits {
        max_face_bytes: bytes.len(),
        ..limits
    });
    assert_eq!(faces.load(bytes.clone(), 0, 20.), Err(UiError::InvalidFont));
    let mut faces = FaceBook::new(FaceLimits {
        max_total_bytes: bytes.len(),
        ..limits
    });
    assert_eq!(faces.load(bytes.clone(), 0, 20.), Err(UiError::FontLimit));
    let mut faces = FaceBook::new(limits);
    let id = faces.load(bytes.clone(), 0, 20.).unwrap();
    assert_eq!(faces.load(bytes.clone(), 0, 20.), Err(UiError::FontLimit));
    faces.unload(id).unwrap();
    faces.load(bytes, 0, 20.).unwrap();
}
