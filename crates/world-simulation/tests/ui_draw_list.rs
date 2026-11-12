//! Immediate overlay fixtures. No GPU or window is created.
#![cfg(feature = "ui")]
#[path = "ui_draw_list/edges.rs"]
mod edges;
#[path = "ui_draw_list/images.rs"]
mod images;
#[path = "ui_draw_list/slices.rs"]
mod slices;
#[path = "ui_draw_list/text.rs"]
mod text;
mod ui_font;

use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::presentation::UiDraw;
use worldrunner_world_simulation::ui::*;

fn fonts() -> FontBook {
    FontBook::new(UiLimits::default()).unwrap()
}
fn list(fonts: &FontBook) -> UiDrawList<'_> {
    UiDrawList::new(
        UVec2::new(100, 80),
        fonts,
        UiOrder {
            layer: 2,
            order: 3,
            first_hierarchy: 5,
        },
        UiDrawLimits::default(),
    )
    .unwrap()
}
fn bounds(draw: &UiDraw) -> [f32; 4] {
    [
        draw.corners[0][0],
        80. - draw.corners[2][1],
        draw.corners[1][0] - draw.corners[0][0],
        draw.corners[2][1] - draw.corners[0][1],
    ]
}

#[test]
fn solid_mapping_and_disjoint_translucent_border() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.solid_rect(UiRect::new(1., 2., 10., 20.), [0.5; 4])
        .unwrap();
    l.border_rect(
        UiRect::new(20., 10., 30., 40.),
        UiBorder {
            width: Vec2::new(2., 3.),
            color: [0.5; 4],
            fill: Some([1.; 4]),
        },
    )
    .unwrap();
    assert_eq!(l.draw_range(), 5..11);
    let p = l.finish().unwrap();
    assert_eq!(
        p.draws[0].corners,
        [[1., 58., 0.], [11., 58., 0.], [11., 78., 0.], [1., 78., 0.]]
    );
    assert_eq!(
        (p.draws[0].uv_rect).map(f32::to_bits),
        ([0., 0., 1., 1.]).map(f32::to_bits)
    );
    assert_eq!(
        p.draws.iter().skip(1).map(bounds).collect::<Vec<_>>(),
        [
            [22., 13., 26., 34.],
            [20., 10., 30., 3.],
            [20., 47., 30., 3.],
            [20., 13., 2., 34.],
            [48., 13., 2., 34.]
        ]
    );
    for (i, draw) in p.draws.iter().enumerate() {
        assert_eq!(
            (draw.sorting_layer, draw.sorting_order, draw.hierarchy_order),
            (2, 3, 5 + i)
        );
        assert_eq!(draw.clip_rect, [0, 0, 100, 80]);
        assert!(!draw.depth_test);
        assert!(draw.triangle_uv.is_none());
    }
}

#[test]
fn small_border_clamps_and_zero_destinations_emit_nothing() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.border_rect(
        UiRect::new(0., 0., 4., 2.),
        UiBorder {
            width: Vec2::splat(10.),
            color: [1.; 4],
            fill: Some([1.; 4]),
        },
    )
    .unwrap();
    l.solid_rect(UiRect::new(0., 0., 0., 10.), [1.; 4]).unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 2);
    assert_eq!(
        (bounds(&p.draws[0])).map(f32::to_bits),
        ([0., 0., 4., 1.]).map(f32::to_bits)
    );
    assert_eq!(
        (bounds(&p.draws[1])).map(f32::to_bits),
        ([0., 1., 4., 1.]).map(f32::to_bits)
    );
}

#[test]
fn nested_float_clips_round_inward_and_restore_without_drift() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.push_clip(UiRect::new(-1., 1.2, 30.8, 20.6)).unwrap();
    l.solid_rect(UiRect::new(0., 0., 100., 80.), [1.; 4])
        .unwrap();
    l.push_clip(UiRect::new(2.2, 3.3, 10.5, 8.5)).unwrap();
    l.solid_rect(UiRect::new(0., 0., 100., 80.), [1.; 4])
        .unwrap();
    l.pop_clip().unwrap();
    l.solid_rect(UiRect::new(0., 0., 100., 80.), [1.; 4])
        .unwrap();
    l.push_clip(UiRect::new(90., 70., 1., 1.)).unwrap();
    l.solid_rect(UiRect::new(0., 0., 100., 80.), [1.; 4])
        .unwrap();
    l.pop_clip().unwrap();
    l.pop_clip().unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 3);
    assert_eq!(p.draws[0].clip_rect, [0, 59, 29, 19]);
    assert_eq!(p.draws[1].clip_rect, [3, 69, 9, 7]);
    assert_eq!(p.draws[2].clip_rect, p.draws[0].clip_rect);
}

#[test]
fn modifiers_multiply_and_restore_for_all_shapes() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.set_color([0.5, 1., 0.5, 0.5]).unwrap();
    l.set_alpha(0.5).unwrap();
    l.push_modifier(UiColorModifier {
        color: [1., 0.5, 1., 0.5],
        alpha: 0.5,
    })
    .unwrap();
    l.solid_rect(UiRect::new(0., 0., 1., 1.), [1.; 4]).unwrap();
    l.pop_modifier().unwrap();
    l.solid_rect(UiRect::new(0., 0., 1., 1.), [1.; 4]).unwrap();
    let p = l.finish().unwrap();
    assert_eq!(
        (p.draws[0].color).map(f32::to_bits),
        ([0.5, 0.5, 0.5, 0.0625]).map(f32::to_bits)
    );
    assert_eq!(
        (p.draws[1].color).map(f32::to_bits),
        ([0.5, 1., 0.5, 0.25]).map(f32::to_bits)
    );
}

#[test]
fn errors_preserve_draws_stacks_and_hierarchy() {
    let fonts = fonts();
    let mut l = UiDrawList::new(
        UVec2::new(100, 80),
        &fonts,
        UiOrder::default(),
        UiDrawLimits {
            max_vertices: 6,
            max_stack_depth: 2,
            ..UiDrawLimits::default()
        },
    )
    .unwrap();
    assert_eq!(l.pop_clip(), Err(UiError::StackUnderflow));
    assert_eq!(l.pop_modifier(), Err(UiError::StackUnderflow));
    l.push_clip(UiRect::new(0., 0., 10., 10.)).unwrap();
    assert_eq!(
        l.push_clip(UiRect::new(0., 0., 1., 1.)),
        Err(UiError::StackLimit)
    );
    l.pop_clip().unwrap();
    assert_eq!(
        l.solid_rect(UiRect::new(0., 0., -1., 1.), [1.; 4]),
        Err(UiError::InvalidRect)
    );
    assert_eq!(l.set_alpha(f32::NAN), Err(UiError::InvalidColor));
    assert_eq!(
        l.border_rect(
            UiRect::new(0., 0., 10., 10.),
            UiBorder {
                width: Vec2::ONE,
                color: [1.; 4],
                fill: None
            }
        ),
        Err(UiError::DrawLimit)
    );
    assert_eq!(l.draw_range(), 0..0);
    l.solid_rect(UiRect::new(0., 0., 1., 1.), [1.; 4]).unwrap();
    assert_eq!(
        l.solid_rect(UiRect::new(0., 0., 1., 1.), [1.; 4]),
        Err(UiError::DrawLimit)
    );
    assert_eq!(l.finish().unwrap().draws.len(), 1);
}

#[test]
fn hierarchy_overflow_and_unbalanced_finish_are_typed() {
    let fonts = fonts();
    let mut l = UiDrawList::new(
        UVec2::new(100, 80),
        &fonts,
        UiOrder {
            first_hierarchy: usize::MAX - 1,
            ..UiOrder::default()
        },
        UiDrawLimits::default(),
    )
    .unwrap();
    l.solid_rect(UiRect::new(0., 0., 1., 1.), [1.; 4]).unwrap();
    assert_eq!(
        l.solid_rect(UiRect::new(0., 0., 1., 1.), [1.; 4]),
        Err(UiError::HierarchyLimit)
    );
    assert_eq!(l.draw_range(), usize::MAX - 1..usize::MAX);
    let mut l = list(&fonts);
    l.push_modifier(UiColorModifier::default()).unwrap();
    assert_eq!(l.finish(), Err(UiError::UnbalancedStack));
}
