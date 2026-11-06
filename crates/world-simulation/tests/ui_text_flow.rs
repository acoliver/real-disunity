#![cfg(feature = "ui")]
mod ui_font;
use glam::{UVec2, Vec2};
use std::sync::Arc;
use worldrunner_world_simulation::ui::*;
use worldrunner_world_simulation::{
    ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace, World, WorldSettings,
};

fn fixture(text: &str) -> (FontBook, TextRun) {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let run = TextRun {
        text: text.into(),
        font_size: 20.,
        vertical_overflow: VerticalOverflow::Overflow,
        ..TextRun::new(font)
    };
    (fonts, run)
}

#[test]
fn paragraphs_wrap_words_break_long_words_and_report_full_height() {
    let (fonts, mut run) = fixture("AA AA\nAAAAA");
    run.line_height = Some(25.);
    run.paragraph_spacing = 7.;
    let layout = fonts.layout(&run, Vec2::new(30., 100.)).unwrap();
    assert_eq!(
        layout.lines.iter().map(|l| l.width).collect::<Vec<_>>(),
        [24., 24., 24., 24., 12.]
    );
    assert_eq!(
        layout.lines.iter().map(|l| l.baseline).collect::<Vec<_>>(),
        [84., 59., 27., 2., -23.]
    );
    assert_eq!(layout.content_height.to_bits(), 127_f32.to_bits());
    run.vertical_overflow = VerticalOverflow::Truncate;
    let short = fonts.layout(&run, Vec2::new(30., 60.)).unwrap();
    assert_eq!(short.lines.len(), 2);
    assert_eq!(short.content_height.to_bits(), 127_f32.to_bits());
}

#[test]
fn alignment_and_blank_paragraphs_keep_their_spacing() {
    let (fonts, mut run) = fixture("A\r\n\nAA");
    run.line_height = Some(24.);
    run.paragraph_spacing = 6.;
    for (horizontal, expected) in [
        (HorizontalAlign::Left, 0_f32),
        (HorizontalAlign::Center, 19.),
        (HorizontalAlign::Right, 38.),
    ] {
        run.horizontal = horizontal;
        let layout = fonts.layout(&run, Vec2::new(50., 100.)).unwrap();
        assert_eq!(layout.lines.len(), 3);
        assert_eq!(layout.glyphs[0].origin.x.to_bits(), expected.to_bits());
        assert_eq!(
            (layout.lines[0].baseline - layout.lines[2].baseline).to_bits(),
            60_f32.to_bits()
        );
        assert_eq!(layout.content_height.to_bits(), 80_f32.to_bits());
    }
    run.text.clear();
    assert_eq!(
        fonts
            .layout(&run, Vec2::splat(100.))
            .unwrap()
            .content_height
            .to_bits(),
        0_f32.to_bits()
    );
}

#[test]
fn styled_ranges_preserve_source_indices_across_consumed_spaces_and_newlines() {
    let (fonts, mut run) = fixture("AA AA\nA");
    run.spans = vec![TextSpan {
        range: 3..7,
        color: [1., 0., 0., 1.],
    }];
    let layout = fonts.layout(&run, Vec2::new(30., 100.)).unwrap();
    assert_eq!(
        layout.glyphs.iter().map(|g| g.color).collect::<Vec<_>>(),
        [
            [1.; 4],
            [1.; 4],
            [1., 0., 0., 1.],
            [1., 0., 0., 1.],
            [1., 0., 0., 1.]
        ]
    );
    for range in [0..99, std::ops::Range { start: 4, end: 2 }] {
        run.spans[0].range = range;
        assert_eq!(
            fonts.layout(&run, Vec2::splat(100.)),
            Err(UiError::InvalidText)
        );
    }
    run.spans = vec![
        TextSpan {
            range: 0..3,
            color: [1.; 4],
        },
        TextSpan {
            range: 2..4,
            color: [1.; 4],
        },
    ];
    assert_eq!(
        fonts.layout(&run, Vec2::splat(100.)),
        Err(UiError::InvalidText)
    );
}

#[test]
fn layout_and_atlas_reuse_storage_and_invalidate_each_layout_input() {
    let (fonts, mut run) = fixture("AA AA");
    let size = Vec2::splat(100.);
    let first = fonts.layout(&run, size).unwrap();
    let stats = fonts.cache_stats();
    for _ in 0..100 {
        let next = fonts.layout(&run, size).unwrap();
        assert!(Arc::ptr_eq(&first, &next));
        assert!(Arc::ptr_eq(&first.atlas.alpha, &next.atlas.alpha));
    }
    assert_eq!(fonts.cache_stats(), stats);
    run.text.push('A');
    let changed = fonts.layout(&run, size).unwrap();
    assert!(!Arc::ptr_eq(&first, &changed));
    run.spans.push(TextSpan {
        range: 0..1,
        color: [0., 1., 0., 1.],
    });
    fonts.layout(&run, size).unwrap();
    run.line_height = Some(30.);
    fonts.layout(&run, size).unwrap();
    fonts.layout(&run, Vec2::new(30., 100.)).unwrap();
    assert_eq!(fonts.cache_stats().layouts, stats.layouts + 4);
    assert_eq!(fonts.cache_stats().atlas_builds, stats.atlas_builds + 4);
}

#[test]
fn missing_latin1_glyphs_and_unsupported_scripts_are_typed_errors() {
    let (fonts, mut run) = fixture("é");
    assert_eq!(
        fonts.layout(&run, Vec2::splat(100.)),
        Err(UiError::MissingGlyph('é'))
    );
    run.text = "中".into();
    assert_eq!(
        fonts.layout(&run, Vec2::splat(100.)),
        Err(UiError::UnsupportedShaping('中'))
    );
    assert_eq!(fonts.cache_stats().layouts, 0);
}

fn rect(world: &mut World, parent: Option<ObjectHandle>, size: Vec2) -> ObjectHandle {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform {
                size_delta: size,
                ..Default::default()
            }),
        )
        .unwrap();
    world
        .set_parent(object, parent, ParentSpace::KeepLocal)
        .unwrap();
    object
}

#[test]
fn extracted_span_colours_scroll_clips_and_unrelated_updates_share_the_atlas() {
    let (fonts, mut run) = fixture("AA\nAA");
    run.spans.push(TextSpan {
        range: 3..5,
        color: [0., 1., 0., 1.],
    });
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = rect(&mut world, None, Vec2::ZERO);
    world
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let view = rect(&mut world, Some(root), Vec2::new(40., 30.));
    let content = rect(&mut world, Some(view), Vec2::new(40., 80.));
    let component = world
        .add_component(content, ComponentDesc::Text(run.clone()))
        .unwrap();
    let unrelated = world.create_object(ObjectDesc::default()).unwrap();
    let mut rules = UiLayoutRules::default();
    rules.scrolls.insert(view, UiScrollView::new(content));
    let extract = |world: &World| {
        contribution::extract_ui_layout(
            world,
            &fonts,
            world.ui_layout_with(UVec2::splat(100), &rules).unwrap(),
        )
        .unwrap()
    };
    let first = extract(&world);
    assert_eq!(first.texts[0].clip_rect, [30, 35, 40, 30]);
    assert_eq!(
        first.texts[0]
            .quads
            .iter()
            .map(|q| q.color)
            .collect::<Vec<_>>(),
        [[1.; 4], [1.; 4], [0., 1., 0., 1.], [0., 1., 0., 1.]]
    );
    let stats = fonts.cache_stats();
    for index in 0..100 {
        world.set_active(unrelated, index % 2 == 0).unwrap();
        let next = extract(&world);
        assert!(Arc::ptr_eq(
            &first.texts[0].texture.as_ref().unwrap().bytes,
            &next.texts[0].texture.as_ref().unwrap().bytes
        ));
    }
    assert_eq!(fonts.cache_stats(), stats);
    run.text.push('A');
    world
        .set_component(component, ComponentDesc::Text(run))
        .unwrap();
    extract(&world);
    assert_eq!(fonts.cache_stats().layouts, stats.layouts + 1);
}

#[test]
fn text_is_clipped_to_its_own_rectangle_even_with_overflow_enabled() {
    let (fonts, run) = fixture("AAAAAAAAAAAAAA");
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = rect(&mut world, None, Vec2::ZERO);
    world
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let text = rect(&mut world, Some(root), Vec2::new(30., 20.));
    world.add_component(text, ComponentDesc::Text(run)).unwrap();
    let plan = contribution::extract_ui(&world, &fonts, [100, 100]).unwrap();
    assert_eq!(plan.texts[0].clip_rect, [35, 40, 30, 20]);
}
