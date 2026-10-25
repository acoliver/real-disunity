#![cfg(feature = "ui")]

#[test]
fn missing_canvas_fails_and_frame_glyph_budget_is_shared() {
    use worldrunner_world_simulation::ParentSpace;
    use worldrunner_world_simulation::ui::{TextRun, UiError};
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    let mut fonts = FontBook::new(UiLimits {
        max_glyphs: 1,
        ..Default::default()
    })
    .unwrap();
    let font = fonts.register(test_font::font()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::Text(TextRun {
                text: "A".into(),
                ..TextRun::new(font)
            }),
        )
        .unwrap();
    assert_eq!(
        extract_ui(&world, &fonts, [800, 600]),
        Err(UiError::MissingLayout.into())
    );
    world
        .add_component(object, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let child = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            child,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .set_parent(child, Some(object), ParentSpace::KeepLocal)
        .unwrap();
    world
        .add_component(
            child,
            ComponentDesc::Text(TextRun {
                text: "A".into(),
                ..TextRun::new(font)
            }),
        )
        .unwrap();
    assert_eq!(
        extract_ui(&world, &fonts, [800, 600]),
        Err(UiError::GlyphLimit.into())
    );
}

#[path = "../../world-simulation/tests/ui_font/mod.rs"]
mod test_font;

#[test]
fn glyph_contributions_reuse_scene_texture_and_preserve_alpha_and_order() {
    use worldrunner_render_plan::{SceneTexture, TexelFormat};
    use worldrunner_world_simulation::ui::{TextRun, UiError};
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    world
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(test_font::font_with_contours(2)).unwrap();
    let text = world
        .add_component(
            root,
            ComponentDesc::Text(TextRun {
                text: "A".into(),
                color: [0.5, 0.25, 1., 0.75],
                ..TextRun::new(font)
            }),
        )
        .unwrap();
    let plan = extract_ui(&world, &fonts, [800, 600]).unwrap();
    assert_eq!(plan.texts.len(), 1);
    let output = &plan.texts[0];
    assert!(output.overlay);
    assert_eq!(
        output.color.map(f32::to_bits),
        [0.5_f32, 0.25, 1., 0.75].map(f32::to_bits)
    );
    assert_eq!(output.quads.len(), 1);
    let texture = output.texture.as_ref().unwrap();
    texture.validate().unwrap();
    assert!(
        matches!(texture, SceneTexture::Texels { format: TexelFormat::R8Unorm, bytes, .. } if bytes.contains(&255))
    );
    assert_eq!(plan, extract_ui(&world, &fonts, [800, 600]).unwrap());
    let empty = FontBook::new(UiLimits::default()).unwrap();
    assert_eq!(
        extract_ui(&world, &empty, [800, 600]),
        Err(UiError::MissingFont(font).into())
    );
    world
        .set_component(
            text,
            ComponentDesc::Text(TextRun {
                enabled: false,
                ..TextRun::new(font)
            }),
        )
        .unwrap();
    assert!(
        extract_ui(&world, &empty, [800, 600])
            .unwrap()
            .texts
            .is_empty()
    );
}

use worldrunner_render_plan::ui::extract_ui;
use worldrunner_world_simulation::ui::{Canvas, FontBook, RectTransform, UiLimits};
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, World, WorldSettings};

#[test]
fn ui_extraction_is_pathless_and_omits_inactive_rectangles() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let root = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    world
        .add_component(root, ComponentDesc::Canvas(Canvas::default()))
        .unwrap();
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan = extract_ui(&world, &fonts, [800, 600]).unwrap();
    assert_eq!(plan.layout.rects.len(), 1);
    assert!(plan.texts.is_empty());
    world.set_active(root, false).unwrap();
    assert!(
        extract_ui(&world, &fonts, [800, 600])
            .unwrap()
            .layout
            .rects
            .is_empty()
    );
}
