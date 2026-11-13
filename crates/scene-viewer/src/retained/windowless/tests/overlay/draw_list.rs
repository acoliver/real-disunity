use super::*;
use std::collections::BTreeMap;
use worldrunner_render_plan::ui::{UiDraw, UiDrawKind, UiPresentation};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};

const COLORS: [[f32; 4]; 4] = [
    [1., 0., 0., 1.],
    [0., 1., 0., 1.],
    [0., 0., 1., 1.],
    [1., 1., 1., 1.],
];

fn draw(depth_test: bool) -> UiDraw {
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test,
        triangle_uv: None,
        corners: [[8., 8., 0.], [24., 8., 0.], [24., 24., 0.], [8., 24., 0.]],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        corner_colors: Some(COLORS),
        texture: Some(7),
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0, 0, 64, 64],
    }
}

fn submit(run: &mut WindowlessRetained<'_>, draws: Vec<UiDraw>) {
    let presentation = UiPresentation {
        draws,
        ..Default::default()
    };
    let textures = BTreeMap::from([(
        7,
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: vec![255; 4].into(),
        },
    )]);
    let overlay = UiOverlay::from_presentation(&presentation, &textures).unwrap();
    run.host_mut()
        .apply(RetainedUpdate::Ui(std::sync::Arc::new(overlay)))
        .unwrap();
}

#[test]
fn corner_tint_pixels() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events).1;
    for depth in [false, true] {
        let quad = draw(depth);
        submit(&mut run, vec![quad.clone()]);
        let image = capture(&mut run, &events).1;
        for (x, y, expected) in [
            (8, 8, [251, 0, 49, 255]),
            (23, 8, [49, 248, 49, 255]),
            (23, 23, [49, 0, 251, 255]),
            (8, 23, [251, 248, 251, 255]),
        ] {
            assert_blended_within_one(pixel(&image, x, y), expected);
        }
        check_pixels(&image, &baseline, &quad);
        let mut tinted = quad.clone();
        tinted.color = [0.5, 0.25, 0.75, 0.5];
        tinted.corner_colors = Some(COLORS.map(|mut c| {
            c[3] = 0.5;
            c
        }));
        submit(&mut run, vec![tinted.clone()]);
        check_pixels(&capture(&mut run, &events).1, &baseline, &tinted);
        uniform_regression(&mut run, &events, quad);
    }
    run.shutdown().unwrap();
}

fn uniform_regression(
    run: &mut WindowlessRetained<'_>,
    events: &RefCell<Vec<RetainedEvent>>,
    mut quad: UiDraw,
) {
    quad.color = [0.5, 0.25, 0.75, 0.5];
    quad.corner_colors = None;
    submit(run, vec![quad.clone()]);
    let uniform = capture(run, events).1;
    quad.corner_colors = Some([[1.; 4]; 4]);
    submit(run, vec![quad.clone()]);
    assert_eq!(capture(run, events).1, uniform);
    let corner = [0.5, 0.25, 1., 0.5];
    quad.corner_colors = Some([corner; 4]);
    submit(run, vec![quad.clone()]);
    let constant = capture(run, events).1;
    quad.color = std::array::from_fn(|c| quad.color[c] * corner[c]);
    quad.corner_colors = None;
    submit(run, vec![quad.clone()]);
    assert_eq!(capture(run, events).1, constant);
    // Same geometry, different corners: adjacent groups must not reuse the first colours.
    let mut back = quad.clone();
    back.corner_colors = Some(COLORS);
    quad.color = [1.; 4];
    quad.hierarchy_order = 1;
    submit(run, vec![back, quad]);
    assert_eq!(pixel(&capture(run, events).1, 16, 16), [255; 4]);
}

fn check_pixels(image: &RgbaImage, baseline: &RgbaImage, quad: &UiDraw) {
    for (x, y) in [
        (8, 8),
        (23, 8),
        (23, 23),
        (8, 23),
        (15, 15),
        (12, 18),
        (18, 12),
    ] {
        let u = (f32::from(u16::try_from(x).unwrap()) + 0.5 - 8.) / 16.;
        let v = (f32::from(u16::try_from(y).unwrap()) + 0.5 - 8.) / 16.;
        let weights = if u >= v {
            [1. - u, u - v, v, 0.]
        } else {
            [1. - v, 0., u, v - u]
        };
        let colors = quad.corner_colors.unwrap();
        let tint: [f32; 4] = std::array::from_fn(|c| {
            quad.color[c] * (0..4).map(|i| weights[i] * colors[i][c]).sum::<f32>()
        });
        let background = pixel(baseline, x, y);
        let expected: [f32; 4] = std::array::from_fn(|c| {
            if c == 3 {
                255.
            } else {
                srgb(tint[c] * tint[3] + linear(background[c]) * (1. - tint[3]))
            }
        });
        for (&actual, expected) in pixel(image, x, y).iter().zip(expected) {
            assert!(
                (f32::from(actual) - expected).abs() <= 1.,
                "pixel {x},{y}: {actual} versus {expected}"
            );
        }
    }
    for (x, y) in [(7, 8), (24, 8), (8, 7), (8, 24), (30, 30)] {
        assert_eq!(pixel(image, x, y), pixel(baseline, x, y));
    }
}
fn linear(byte: u8) -> f32 {
    let v = f32::from(byte) / 255.;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
fn srgb(v: f32) -> f32 {
    let v = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    };
    (v * 255.).round()
}
