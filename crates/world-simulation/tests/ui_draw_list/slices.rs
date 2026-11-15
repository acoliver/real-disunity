use super::*;

fn limited_list(fonts: &FontBook, max_vertices: usize) -> UiDrawList<'_> {
    UiDrawList::new(
        UVec2::new(100, 80),
        fonts,
        UiOrder::default(),
        UiDrawLimits {
            max_vertices,
            ..UiDrawLimits::default()
        },
    )
    .unwrap()
}

#[test]
fn compressed_tiles_share_one_seam_with_omitted_center() {
    compressed_tiles_share_one_seam(UiSliceCenter::Omit);
}

#[test]
fn compressed_tiles_share_one_seam_with_drawn_center() {
    compressed_tiles_share_one_seam(UiSliceCenter::Draw);
}

fn compressed_tiles_share_one_seam(center: UiSliceCenter) {
    let fonts = fonts();
    let mut l = limited_list(&fonts, 24);
    l.nine_slice(
        UiImage::new(UiImageId(7), UVec2::splat(6)).unwrap(),
        UiRect::new(0., 0., 5., 5.),
        UiNineSlice {
            insets: UiInsets {
                left: 1.,
                top: 1.,
                right: 5.,
                bottom: 5.,
            },
            mode: UiSliceMode::Tile,
            center,
            ..UiNineSlice::default()
        },
    )
    .unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 4);
    assert_eq!(
        p.draws[0].corners[1][0].to_bits(),
        p.draws[1].corners[0][0].to_bits()
    );
    assert_eq!(
        p.draws[0].corners[0][1].to_bits(),
        p.draws[2].corners[3][1].to_bits()
    );
    for (index, d) in p.draws.iter().enumerate() {
        assert!(bounds(d)[2] > 0. && bounds(d)[3] > 0.);
        assert_eq!(d.hierarchy_order, index);
    }
    assert_eq!(p.draws[0].corners[0][0].to_bits(), 0_f32.to_bits());
    assert_eq!(p.draws[3].corners[1][0].to_bits(), 5_f32.to_bits());
    assert_eq!(p.draws[0].corners[3][1].to_bits(), 80_f32.to_bits());
    assert_eq!(p.draws[3].corners[0][1].to_bits(), 75_f32.to_bits());
}

fn fractional_tiles(l: &mut UiDrawList<'_>) -> Result<(), UiError> {
    l.nine_slice(
        UiImage::new(UiImageId(7), UVec2::ONE).unwrap(),
        UiRect::new(0., 0., 0.3, 1.),
        UiNineSlice {
            scale: Vec2::new(0.1, 1.),
            mode: UiSliceMode::Tile,
            ..UiNineSlice::default()
        },
    )
}

#[test]
fn fractional_tile_count_matches_nonempty_emission() {
    let fonts = fonts();
    let mut l = list(&fonts);
    fractional_tiles(&mut l).unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 3);
    for (i, d) in p.draws.iter().enumerate() {
        assert!(bounds(d)[2] > 0. && bounds(d)[3] > 0.);
        assert_eq!(d.hierarchy_order, 5 + i);
    }
    assert_eq!(p.draws[0].corners[0][0].to_bits(), 0_f32.to_bits());
    assert_eq!(p.draws[2].corners[1][0].to_bits(), 0.3_f32.to_bits());
    for pair in p.draws.windows(2) {
        assert_eq!(
            pair[0].corners[1][0].to_bits(),
            pair[1].corners[0][0].to_bits()
        );
    }
}

#[test]
fn fractional_tiles_fit_exact_vertex_budget() {
    let fonts = fonts();
    let mut l = limited_list(&fonts, 18);
    fractional_tiles(&mut l).unwrap();
    assert_eq!(l.finish().unwrap().draws.len(), 3);
    let mut l = limited_list(&fonts, 17);
    assert_eq!(fractional_tiles(&mut l), Err(UiError::DrawLimit));
    assert!(l.finish().unwrap().draws.is_empty());
}

#[test]
fn large_omitted_frame_expands_only_border_draws() {
    let fonts = fonts();
    let mut l = UiDrawList::new(
        UVec2::splat(5461),
        &fonts,
        UiOrder::default(),
        UiDrawLimits {
            max_vertices: 10921 * 6,
            ..UiDrawLimits::default()
        },
    )
    .unwrap();
    l.nine_slice(
        UiImage::new(UiImageId(7), UVec2::splat(2)).unwrap(),
        UiRect::new(0., 0., 5461., 5461.),
        UiNineSlice {
            insets: UiInsets {
                left: 1.,
                top: 1.,
                ..UiInsets::default()
            },
            mode: UiSliceMode::Tile,
            center: UiSliceCenter::Omit,
            ..UiNineSlice::default()
        },
    )
    .unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 10921);
    for (i, d) in p.draws.iter().enumerate() {
        assert_eq!(d.hierarchy_order, i);
        let expected = if i < 5461 {
            [u16::try_from(i).unwrap(), 0]
        } else {
            [0, u16::try_from(i - 5460).unwrap()]
        };
        assert_eq!(
            d.corners[3].map(f32::to_bits),
            [f32::from(expected[0]), 5461. - f32::from(expected[1]), 0.].map(f32::to_bits)
        );
    }
}

#[test]
fn omitted_tiles_preserve_drawn_frame_border_bytes_and_order() {
    let fonts = fonts();
    let image = UiImage::new(UiImageId(7), UVec2::splat(2)).unwrap();
    for flip in [UiFlip::None, UiFlip::X, UiFlip::Y, UiFlip::XY] {
        let style = UiNineSlice {
            insets: UiInsets {
                left: 1.,
                top: 1.,
                ..UiInsets::default()
            },
            mode: UiSliceMode::Tile,
            flip,
            ..UiNineSlice::default()
        };
        let mut full = list(&fonts);
        full.nine_slice(image, UiRect::new(0., 0., 5., 4.), style)
            .unwrap();
        let mut expected = full.finish().unwrap();
        expected.draws.retain(|d| {
            d.uv_rect[0] == 0. || d.uv_rect[1] == 0. || d.uv_rect[2] == 0. || d.uv_rect[3] == 0.
        });
        for (i, d) in expected.draws.iter_mut().enumerate() {
            d.hierarchy_order = 5 + i;
        }
        let mut omitted = list(&fonts);
        omitted
            .nine_slice(
                image,
                UiRect::new(0., 0., 5., 4.),
                UiNineSlice {
                    center: UiSliceCenter::Omit,
                    ..style
                },
            )
            .unwrap();
        assert_eq!(omitted.finish().unwrap(), expected, "{flip:?}");
    }
}
