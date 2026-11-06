#![cfg(feature = "ui")]
mod ui_font;
use glam::{UVec2, Vec2};
use std::sync::Arc;
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, ParentSpace, World, WorldSettings};

fn latin_font() -> Vec<u8> {
    let mut bytes = ui_font::font();
    let record = (12..12 + 7 * 16)
        .step_by(16)
        .find(|&i| &bytes[i..i + 4] == b"cmap")
        .unwrap();
    let cmap = usize::try_from(u32::from_be_bytes(
        bytes[record + 8..record + 12].try_into().unwrap(),
    ))
    .unwrap()
        + 12;
    // Remap the last printable ASCII segment to é, preserving sorted format-4 segments.
    let end = cmap + 14 + 94 * 2;
    let start = cmap + 16 + 96 * 2 + 94 * 2;
    let delta = cmap + 16 + 96 * 4 + 94 * 2;
    bytes[end..end + 2].copy_from_slice(&233_u16.to_be_bytes());
    bytes[start..start + 2].copy_from_slice(&233_u16.to_be_bytes());
    bytes[delta..delta + 2].copy_from_slice(&1_u16.wrapping_sub(233).to_be_bytes());
    bytes
}

#[test]
fn available_latin1_glyphs_keep_utf8_ranges_through_word_wrapping() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(latin_font()).unwrap();
    let run = TextRun {
        text: "A é".into(),
        font_size: 20.,
        spans: vec![TextSpan {
            link: None,
            range: 2..4,
            color: [0., 1., 0., 1.],
        }],
        ..TextRun::new(id)
    };
    let layout = fonts.layout(&run, Vec2::new(24., 100.)).unwrap();
    assert_eq!(layout.lines.len(), 2);
    assert_eq!(
        layout
            .glyphs
            .iter()
            .map(|g| g.character)
            .collect::<String>(),
        "Aé"
    );
    assert_eq!(
        layout.glyphs[1].color.map(f32::to_bits),
        [0., 1., 0., 1.].map(f32::to_bits)
    );
}

#[test]
fn cache_eviction_keeps_detached_layouts_valid_and_enforces_input_limits() {
    let mut fonts = FontBook::new(UiLimits {
        max_glyphs: 2,
        ..UiLimits::default()
    })
    .unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    let mut run = TextRun {
        text: "AA".into(),
        ..TextRun::new(id)
    };
    let first = fonts.layout(&run, Vec2::splat(100.)).unwrap();
    run.text = "B".into();
    fonts.layout(&run, Vec2::splat(100.)).unwrap();
    run.text = "AA".into();
    let next = fonts.layout(&run, Vec2::splat(100.)).unwrap();
    assert!(!Arc::ptr_eq(&first, &next));
    assert_eq!(first, next);
    assert_eq!(fonts.cache_stats().layouts, 3);
    run.text = "\r\r\r".into();
    assert_eq!(
        fonts.layout(&run, Vec2::splat(100.)),
        Err(UiError::GlyphLimit)
    );
    assert_eq!(fonts.cache_stats().layouts, 3);
}

#[test]
fn invalid_paragraph_settings_and_utf8_span_boundaries_fail_explicitly() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    let mut run = TextRun::new(id);
    for height in [0., -1., f32::INFINITY, f32::NAN] {
        run.line_height = Some(height);
        assert_eq!(
            fonts.layout(&run, Vec2::splat(100.)),
            Err(UiError::InvalidText)
        );
    }
    run.line_height = None;
    for spacing in [-1., f32::INFINITY, f32::NAN] {
        run.paragraph_spacing = spacing;
        assert_eq!(
            fonts.layout(&run, Vec2::splat(100.)),
            Err(UiError::InvalidText)
        );
    }
    run.paragraph_spacing = 0.;
    run.text = "é".into();
    run.spans.push(TextSpan {
        link: None,
        range: 0..1,
        color: [1.; 4],
    });
    assert_eq!(
        fonts.layout(&run, Vec2::splat(100.)),
        Err(UiError::InvalidText)
    );
    assert_eq!(fonts.cache_stats().layouts, 0);
}

#[test]
fn measured_paragraph_height_sizes_and_scrolls_the_existing_content_rectangle() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    let run = TextRun {
        text: "AA AA\nAAAAA".into(),
        font_size: 20.,
        line_height: Some(25.),
        paragraph_spacing: 7.,
        vertical_overflow: VerticalOverflow::Overflow,
        ..TextRun::new(id)
    };
    let measured = fonts.layout(&run, Vec2::new(30., 0.)).unwrap();
    let mut world = World::new(WorldSettings::default()).unwrap();
    let viewport = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            viewport,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .add_component(viewport, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let content = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(content, Some(viewport), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(
            content,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .add_component(content, ComponentDesc::Text(run))
        .unwrap();
    let mut rules = UiLayoutRules::default();
    rules.sizes.insert(
        content,
        UiSize::Fixed(Vec2::new(30., measured.content_height.ceil()).as_uvec2()),
    );
    rules.scrolls.insert(viewport, UiScrollView::new(content));
    let layout = world.ui_layout_with(UVec2::new(30, 60), &rules).unwrap();
    assert_eq!(layout.rect(content).unwrap().size, Vec2::new(30., 127.));
    rules
        .scroll_to(&layout, viewport, Vec2::new(0., 999.))
        .unwrap();
    assert_eq!(rules.scrolls[&viewport].offset(), Vec2::new(0., 67.));
    let layout = world.ui_layout_with(UVec2::new(30, 60), &rules).unwrap();
    let plan = contribution::extract_ui_layout(&world, &fonts, layout).unwrap();
    assert_eq!(plan.texts[0].clip_rect, [0, 0, 30, 60]);
}

fn pixel_perfect_text() -> (World, worldrunner_world_simulation::ObjectHandle, FontBook) {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let id = fonts.register(ui_font::font()).unwrap();
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    world
        .add_component(
            root,
            ComponentDesc::Canvas(Canvas {
                pixel_perfect: true,
                ..Canvas::default()
            }),
        )
        .unwrap();
    let text = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(text, Some(root), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(
            text,
            ComponentDesc::RectTransform(RectTransform {
                size_delta: Vec2::new(20., 20.),
                anchored_position: Vec2::splat(0.37),
                ..RectTransform::default()
            }),
        )
        .unwrap();
    world
        .add_component(
            text,
            ComponentDesc::Text(TextRun {
                text: "AAA".into(),
                font_size: 20.,
                horizontal_overflow: HorizontalOverflow::Overflow,
                ..TextRun::new(id)
            }),
        )
        .unwrap();
    (world, text, fonts)
}

#[test]
fn pixel_snapping_cannot_move_rotated_glyphs_outside_their_local_rectangle() {
    let (mut world, text, fonts) = pixel_perfect_text();
    for angle in [0.3, -0.3] {
        world
            .set_local_transform(
                text,
                worldrunner_world_simulation::LocalTransform {
                    rotation: glam::Quat::from_rotation_z(angle),
                    ..Default::default()
                },
            )
            .unwrap();
        let plan = contribution::extract_ui(&world, &fonts, [100, 100]).unwrap();
        let rect = plan.layout.rect(text).unwrap();
        assert!(!plan.texts[0].quads.is_empty());
        for corner in plan.texts[0].quads.iter().flat_map(|q| q.corners) {
            let local = rect
                .local_to_world
                .inverse()
                .transform_point3(corner.into())
                .truncate()
                + rect.pivot * rect.size;
            assert!(
                local.cmpge(Vec2::splat(-0.0001)).all()
                    && local.cmple(rect.size + Vec2::splat(0.0001)).all(),
                "local corner {local:?}"
            );
        }
    }
}
