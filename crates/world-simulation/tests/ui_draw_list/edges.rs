use super::*;

#[test]
fn omitted_unbordered_frame_has_no_tile_expansion() {
    let fonts = fonts();
    let mut l = UiDrawList::new(
        UVec2::new(100, 80),
        &fonts,
        UiOrder::default(),
        UiDrawLimits {
            max_vertices: 0,
            ..UiDrawLimits::default()
        },
    )
    .unwrap();
    l.nine_slice(
        images::image(),
        UiRect::new(0., 0., 90., 70.),
        UiNineSlice {
            mode: UiSliceMode::Tile,
            center: UiSliceCenter::Omit,
            scale: Vec2::splat(0.000_001),
            ..UiNineSlice::default()
        },
    )
    .unwrap();
    assert!(l.finish().unwrap().draws.is_empty());
}

#[test]
fn image_frame_border_and_text_inherit_modifiers_and_scissor() {
    let (fonts, run) = text::fixture();
    let mut l = list(&fonts);
    l.push_clip(UiRect::new(0., 0., 50., 40.)).unwrap();
    l.push_modifier(UiColorModifier {
        color: [0.5; 4],
        alpha: 0.5,
    })
    .unwrap();
    let rect = UiRect::new(0., 0., 20., 20.);
    l.image(images::image(), rect, UiImageOptions::default())
        .unwrap();
    l.border_rect(
        rect,
        UiBorder {
            width: Vec2::ONE,
            color: [1.; 4],
            fill: Some([1.; 4]),
        },
    )
    .unwrap();
    l.nine_slice(images::image(), rect, UiNineSlice::default())
        .unwrap();
    l.text(&run, UiTextPlacement::new(rect), UiTextEffect::None)
        .unwrap();
    l.pop_modifier().unwrap();
    l.pop_clip().unwrap();
    let p = l.finish().unwrap();
    assert!(!p.draws.is_empty());
    for d in p.draws {
        assert_eq!(
            d.color.map(f32::to_bits),
            [0.5_f32, 0.5, 0.5, 0.25].map(f32::to_bits)
        );
        assert_eq!(d.clip_rect, [0, 40, 50, 40]);
    }
}

#[test]
fn root_inclusive_modifier_limit_and_clip_finish_are_checked() {
    let fonts = fonts();
    let mut l = UiDrawList::new(
        UVec2::new(100, 80),
        &fonts,
        UiOrder::default(),
        UiDrawLimits {
            max_stack_depth: 1,
            ..UiDrawLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        l.push_modifier(UiColorModifier::default()),
        Err(UiError::StackLimit)
    );
    assert_eq!(
        l.push_clip(UiRect::new(0., 0., 1., 1.)),
        Err(UiError::StackLimit)
    );
    assert_eq!(
        l.push_modifier(UiColorModifier {
            alpha: f32::NAN,
            ..UiColorModifier::default()
        }),
        Err(UiError::InvalidColor)
    );
    assert!(l.finish().is_ok());
    let mut l = list(&fonts);
    l.push_clip(UiRect::new(0., 0., 1., 1.)).unwrap();
    assert_eq!(l.finish(), Err(UiError::UnbalancedStack));
}

#[test]
fn append_remaps_only_glyph_ids_and_preserves_image_namespace_and_order() {
    let (fonts, run) = text::fixture();
    let mut combined = worldrunner_world_simulation::ui::presentation::UiPresentation::default();
    let image = UiImage::new(UiImageId(0), UVec2::new(16, 12)).unwrap();
    for first_hierarchy in [0, 3] {
        let mut l = UiDrawList::new(
            UVec2::new(100, 80),
            &fonts,
            UiOrder {
                first_hierarchy,
                ..UiOrder::default()
            },
            UiDrawLimits::default(),
        )
        .unwrap();
        l.image(
            image,
            UiRect::new(0., 0., 10., 10.),
            UiImageOptions::default(),
        )
        .unwrap();
        l.text(
            &run,
            UiTextPlacement::new(UiRect::new(0., 0., 60., 40.)),
            UiTextEffect::None,
        )
        .unwrap();
        combined.append(l.finish().unwrap()).unwrap();
    }
    assert_eq!(combined.coverage.len(), 2);
    assert_eq!(
        combined.draws.iter().map(|d| d.texture).collect::<Vec<_>>(),
        [
            Some(UiTexture::Image(0)),
            Some(UiTexture::Coverage(0)),
            Some(UiTexture::Coverage(0)),
            Some(UiTexture::Image(0)),
            Some(UiTexture::Coverage(1)),
            Some(UiTexture::Coverage(1)),
        ]
    );
    assert_eq!(
        combined
            .draws
            .iter()
            .map(|d| d.hierarchy_order)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5]
    );
}
