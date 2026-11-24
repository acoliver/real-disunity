use super::*;

pub(super) fn image() -> UiImage {
    UiImage::new(UiImageId(7), UVec2::new(16, 12)).unwrap()
}
fn frame(mode: UiSliceMode, flip: UiFlip) -> UiNineSlice {
    UiNineSlice {
        source: None,
        insets: UiInsets {
            left: 2.,
            top: 1.,
            right: 4.,
            bottom: 3.,
        },
        scale: Vec2::ONE,
        mode,
        center: UiSliceCenter::Draw,
        color: [1.; 4],
        flip,
    }
}

#[test]
fn images_keep_corner_tints_and_flip_only_uvs() {
    let fonts = fonts();
    let mut l = list(&fonts);
    let colors = [
        [1., 0., 0., 1.],
        [0., 1., 0., 1.],
        [0., 0., 1., 1.],
        [1.; 4],
    ];
    for flip in [UiFlip::None, UiFlip::X, UiFlip::Y, UiFlip::XY] {
        l.image(
            image(),
            UiRect::new(10., 20., 8., 6.),
            UiImageOptions {
                source: Some(UiRect::new(2., 1., 8., 6.)),
                colors,
                flip,
                ..UiImageOptions::default()
            },
        )
        .unwrap();
    }
    let p = l.finish().unwrap();
    for (draw, uv) in p.draws.iter().zip([
        [2., 1., 10., 7.],
        [10., 1., 2., 7.],
        [2., 7., 10., 1.],
        [10., 7., 2., 1.],
    ]) {
        assert_eq!(
            (draw.uv_rect).map(f32::to_bits),
            ([uv[0] / 16., uv[1] / 12., uv[2] / 16., uv[3] / 12.]).map(f32::to_bits)
        );
        assert_eq!(draw.corner_colors, Some(colors));
        assert_eq!(draw.texture, Some(UiTexture::Image(7)));
        assert_eq!(
            (bounds(draw)).map(f32::to_bits),
            ([10., 20., 8., 6.]).map(f32::to_bits)
        );
    }
}

#[test]
fn image_rotation_is_clockwise_about_destination_pivot() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.image(
        image(),
        UiRect::new(10., 20., 8., 6.),
        UiImageOptions {
            rotation_radians: std::f32::consts::FRAC_PI_2,
            pivot: Vec2::ZERO,
            ..UiImageOptions::default()
        },
    )
    .unwrap();
    let p = l.finish().unwrap();
    for (a, b) in p.draws[0].corners.iter().zip([
        [4., 60., 0.],
        [4., 52., 0.],
        [10., 52., 0.],
        [10., 60., 0.],
    ]) {
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 0.00001);
        }
    }
}

#[test]
fn stretch_frame_omits_center_and_compresses_borders() {
    let fonts = fonts();
    let mut l = list(&fonts);
    l.nine_slice(
        image(),
        UiRect::new(0., 0., 31., 21.),
        frame(UiSliceMode::Stretch, UiFlip::None),
    )
    .unwrap();
    let mut style = frame(UiSliceMode::Stretch, UiFlip::None);
    style.center = UiSliceCenter::Omit;
    l.nine_slice(image(), UiRect::new(0., 0., 31., 21.), style)
        .unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 17);
    assert_eq!(
        (bounds(&p.draws[4])).map(f32::to_bits),
        ([2., 1., 25., 17.]).map(f32::to_bits)
    );
    let mut l = list(&fonts);
    l.nine_slice(image(), UiRect::new(0., 0., 3., 2.), style)
        .unwrap();
    let p = l.finish().unwrap();
    assert_eq!(p.draws.len(), 4);
    assert_eq!(
        (bounds(&p.draws[0])).map(f32::to_bits),
        ([0., 0., 1., 0.5]).map(f32::to_bits)
    );
    assert_eq!(
        (bounds(&p.draws[3])).map(f32::to_bits),
        ([1., 0.5, 2., 1.5]).map(f32::to_bits)
    );
}

#[test]
fn asymmetric_tiles_mirror_partial_phase_and_border_widths() {
    let fonts = fonts();
    for (flip, xs, us, ys, vs) in [
        (
            UiFlip::None,
            vec![[2., 12.], [12., 22.], [22., 27.]],
            vec![[2., 12.], [2., 12.], [2., 7.]],
            vec![[1., 9.], [9., 17.], [17., 18.]],
            vec![[1., 9.], [1., 9.], [1., 2.]],
        ),
        (
            UiFlip::X,
            vec![[4., 9.], [9., 19.], [19., 29.]],
            vec![[7., 2.], [12., 2.], [12., 2.]],
            vec![[1., 9.], [9., 17.], [17., 18.]],
            vec![[1., 9.], [1., 9.], [1., 2.]],
        ),
        (
            UiFlip::Y,
            vec![[2., 12.], [12., 22.], [22., 27.]],
            vec![[2., 12.], [2., 12.], [2., 7.]],
            vec![[3., 4.], [4., 12.], [12., 20.]],
            vec![[2., 1.], [9., 1.], [9., 1.]],
        ),
        (
            UiFlip::XY,
            vec![[4., 9.], [9., 19.], [19., 29.]],
            vec![[7., 2.], [12., 2.], [12., 2.]],
            vec![[3., 4.], [4., 12.], [12., 20.]],
            vec![[2., 1.], [9., 1.], [9., 1.]],
        ),
    ] {
        let mut l = list(&fonts);
        l.nine_slice(
            image(),
            UiRect::new(0., 0., 31., 21.),
            frame(UiSliceMode::Tile, flip),
        )
        .unwrap();
        let p = l.finish().unwrap();
        assert_eq!(p.draws.len(), 25);
        for (iy, (y, v)) in ys.iter().zip(&vs).enumerate() {
            for (ix, (x, u)) in xs.iter().zip(&us).enumerate() {
                let d = &p.draws[(iy + 1) * 5 + ix + 1];
                assert_eq!(
                    (bounds(d)).map(f32::to_bits),
                    ([x[0], y[0], x[1] - x[0], y[1] - y[0]]).map(f32::to_bits),
                    "{flip:?}"
                );
                assert_eq!(
                    (d.uv_rect).map(f32::to_bits),
                    ([u[0] / 16., v[0] / 12., u[1] / 16., v[1] / 12.]).map(f32::to_bits)
                );
            }
        }
        for (i, d) in p.draws.iter().enumerate() {
            assert_eq!(d.hierarchy_order, 5 + i);
        }
        if matches!(flip, UiFlip::X | UiFlip::XY) {
            assert_eq!(f32::to_bits(bounds(&p.draws[0])[2]), f32::to_bits(4.));
            assert_eq!(f32::to_bits(p.draws[0].uv_rect[0]), f32::to_bits(1.));
            assert_eq!(f32::to_bits(p.draws[4].uv_rect[2]), f32::to_bits(0.));
        }
    }
}

#[test]
fn invalid_sources_tiles_and_expansion_fail_atomically() {
    let fonts = fonts();
    let mut l = list(&fonts);
    assert_eq!(
        l.image(
            image(),
            UiRect::new(0., 0., 1., 1.),
            UiImageOptions {
                source: Some(UiRect::new(15., 0., 2., 2.)),
                ..UiImageOptions::default()
            }
        ),
        Err(UiError::InvalidImage)
    );
    let mut style = frame(UiSliceMode::Tile, UiFlip::None);
    style.insets.right = 14.;
    assert_eq!(
        l.nine_slice(image(), UiRect::new(0., 0., 31., 21.), style),
        Err(UiError::InvalidImage)
    );
    style = frame(UiSliceMode::Tile, UiFlip::None);
    style.scale = Vec2::splat(0.000_001);
    assert_eq!(
        l.nine_slice(image(), UiRect::new(0., 0., 31., 21.), style),
        Err(UiError::DrawLimit)
    );
    assert_eq!(l.draw_range(), 5..5);
    assert!(l.finish().unwrap().draws.is_empty());
}
