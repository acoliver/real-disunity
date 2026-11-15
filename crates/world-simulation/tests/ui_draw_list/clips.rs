use super::*;

fn draw(list: &mut UiDrawList<'_>) {
    list.solid_rect(UiRect::new(0., 0., 100., 80.), [1.; 4])
        .unwrap();
}

#[test]
fn intersect_replace_intersect_restores_each_scissor_on_pop() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.push_clip(UiRect::new(10.2, 12.3, 30.6, 40.5)).unwrap();
    draw(&mut l);
    l.push_clip_replace(UiRect::new(25.2, 4.3, 60.6, 65.5))
        .unwrap();
    draw(&mut l);
    l.push_clip(UiRect::new(20., 20., 30., 50.)).unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    let p = l.finish().unwrap();
    assert_eq!(
        p.draws.iter().map(|d| d.clip_rect).collect::<Vec<_>>(),
        [
            [11, 28, 29, 39],
            [26, 11, 59, 64],
            [26, 11, 24, 49],
            [26, 11, 59, 64],
            [11, 28, 29, 39],
            [0, 0, 100, 80],
        ]
    );
}

#[test]
fn replace_intersect_replace_restores_each_scissor_on_pop() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.push_clip_replace(UiRect::new(10.2, 12.3, 30.6, 40.5))
        .unwrap();
    draw(&mut l);
    l.push_clip(UiRect::new(25.2, 4.3, 60.6, 65.5)).unwrap();
    draw(&mut l);
    l.push_clip_replace(UiRect::new(20., 20., 30., 50.))
        .unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    let p = l.finish().unwrap();
    assert_eq!(
        p.draws.iter().map(|d| d.clip_rect).collect::<Vec<_>>(),
        [
            [11, 28, 29, 39],
            [26, 28, 14, 39],
            [20, 10, 30, 50],
            [26, 28, 14, 39],
            [11, 28, 29, 39],
            [0, 0, 100, 80],
        ]
    );
}

#[test]
fn zero_width_and_zero_height_replacements_hide_draws_and_restore_parent() {
    let fonts = fonts();
    for rect in [
        UiRect::new(25., 20., 0., 30.),
        UiRect::new(20., 25., 30., 0.),
    ] {
        let mut l = list(&fonts);
        l.push_clip(UiRect::new(10.2, 12.3, 30.6, 40.5)).unwrap();
        draw(&mut l);
        assert_eq!(l.draw_range(), 5..6);
        l.push_clip_replace(rect).unwrap();
        draw(&mut l);
        assert_eq!(l.draw_range(), 5..6, "replacement {rect:?}");
        l.pop_clip().unwrap();
        draw(&mut l);
        assert_eq!(l.draw_range(), 5..7);
        l.pop_clip().unwrap();
        draw(&mut l);
        let p = l.finish().unwrap();
        assert_eq!(
            p.draws.iter().map(|d| d.clip_rect).collect::<Vec<_>>(),
            [[11, 28, 29, 39], [11, 28, 29, 39], [0, 0, 100, 80]],
            "replacement {rect:?}"
        );
    }
}

#[test]
fn replace_inside_intersecting_parent_and_pop_restore_each_scissor() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.push_clip(UiRect::new(10., 10., 50., 60.)).unwrap();
    l.push_clip(UiRect::new(20., 0., 30., 80.)).unwrap();
    draw(&mut l);
    l.push_clip_replace(UiRect::new(40., 5., 50., 70.)).unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    let p = l.finish().unwrap();
    assert_eq!(
        p.draws.iter().map(|d| d.clip_rect).collect::<Vec<_>>(),
        [
            [20, 10, 30, 60],
            [40, 5, 50, 70],
            [20, 10, 30, 60],
            [10, 10, 50, 60],
            [0, 0, 100, 80],
        ]
    );
}

#[test]
fn nested_replace_restores_float_rectangles_without_rounding_drift() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.push_clip_replace(UiRect::new(1.2, 2.3, 30.6, 40.5))
        .unwrap();
    draw(&mut l);
    l.push_clip_replace(UiRect::new(50.2, 4.3, 40.6, 60.5))
        .unwrap();
    draw(&mut l);
    l.push_clip(UiRect::new(40., 10., 20., 50.)).unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    let p = l.finish().unwrap();
    assert_eq!(
        p.draws.iter().map(|d| d.clip_rect).collect::<Vec<_>>(),
        [
            [2, 38, 29, 39],
            [51, 16, 39, 59],
            [51, 20, 9, 50],
            [51, 16, 39, 59],
            [2, 38, 29, 39],
        ]
    );
}

#[test]
fn replace_clamps_to_viewport_and_can_escape_an_empty_parent() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.push_clip(UiRect::new(200., 200., 10., 10.)).unwrap();
    draw(&mut l);
    l.push_clip_replace(UiRect::new(-20., 10., 150., 100.))
        .unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    l.push_clip_replace(UiRect::new(120., 0., 10., 10.))
        .unwrap();
    draw(&mut l);
    l.pop_clip().unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 1);
    assert_eq!(p.draws[0].clip_rect, [0, 0, 100, 70]);
}

#[test]
fn replace_and_pop_preserve_quad_geometry_colors_and_painter_order() {
    let fonts = fonts();
    let mut l = list(&fonts);
    let rect = UiRect::new(1., 2., 90., 70.);
    l.push_clip(UiRect::new(10., 10., 20., 20.)).unwrap();
    l.solid_rect(rect, [1., 0., 0., 1.]).unwrap();
    l.push_clip_replace(UiRect::new(20., 20., 60., 40.))
        .unwrap();
    l.solid_rect(rect, [0., 1., 0., 1.]).unwrap();
    l.pop_clip().unwrap();
    l.solid_rect(rect, [0., 0., 1., 1.]).unwrap();
    l.pop_clip().unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 3);
    let expected = [
        ([1_f32, 0., 0., 1.], [10, 50, 20, 20]),
        ([0., 1., 0., 1.], [20, 20, 60, 40]),
        ([0., 0., 1., 1.], [10, 50, 20, 20]),
    ];
    for (i, (draw, (color, clip))) in p.draws.iter().zip(expected).enumerate() {
        assert_eq!(
            bounds(draw).map(f32::to_bits),
            [1_f32, 2., 90., 70.].map(f32::to_bits)
        );
        assert_eq!(draw.color.map(f32::to_bits), color.map(f32::to_bits));
        assert_eq!(draw.clip_rect, clip);
        assert_eq!(
            (draw.sorting_layer, draw.sorting_order, draw.hierarchy_order),
            (2, 3, 5 + i)
        );
    }
}

#[test]
fn replace_errors_do_not_mutate_the_clip_stack() {
    let fonts = fonts();
    let mut l = UiDrawList::new(
        UVec2::new(100, 80),
        &fonts,
        UiOrder::default(),
        UiDrawLimits {
            max_stack_depth: 2,
            ..UiDrawLimits::default()
        },
    )
    .unwrap();
    for rect in [
        UiRect::new(f32::NAN, 0., 1., 1.),
        UiRect::new(0., 0., -1., 1.),
    ] {
        assert_eq!(l.push_clip_replace(rect), Err(UiError::InvalidRect));
    }
    l.push_clip_replace(UiRect::new(10., 10., 20., 20.))
        .unwrap();
    assert_eq!(
        l.push_clip_replace(UiRect::new(0., 0., 1., 1.)),
        Err(UiError::StackLimit)
    );
    draw(&mut l);
    l.pop_clip().unwrap();
    draw(&mut l);
    let p = l.finish().unwrap();
    assert_eq!(p.draws[0].clip_rect, [10, 50, 20, 20]);
    assert_eq!(p.draws[1].clip_rect, [0, 0, 100, 80]);
}
