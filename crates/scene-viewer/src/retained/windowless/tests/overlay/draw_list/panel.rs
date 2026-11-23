use super::*;
use glam::{UVec2, Vec2};
use worldrunner_world_simulation::ui::*;

fn panel(fonts: &FontBook, run: &TextRun, effect: UiTextEffect) -> UiPresentation {
    let mut l = UiDrawList::new(
        UVec2::splat(64),
        fonts,
        UiOrder::default(),
        UiDrawLimits::default(),
    )
    .unwrap();
    l.solid_rect(UiRect::new(4., 4., 56., 56.), [0., 0., 0., 1.])
        .unwrap();
    let frame = UiImage::new(UiImageId(1), UVec2::splat(4)).unwrap();
    l.nine_slice(
        frame,
        UiRect::new(4., 4., 56., 56.),
        UiNineSlice {
            insets: UiInsets {
                left: 1.,
                top: 1.,
                right: 1.,
                bottom: 1.,
            },
            scale: Vec2::splat(2.),
            center: UiSliceCenter::Omit,
            ..UiNineSlice::default()
        },
    )
    .unwrap();
    let checker = UiImage::new(UiImageId(2), UVec2::splat(2)).unwrap();
    l.image(
        checker,
        UiRect::new(44., 12., 8., 8.),
        UiImageOptions::default(),
    )
    .unwrap();
    l.push_clip(UiRect::new(10., 10., 44., 44.)).unwrap();
    l.push_clip(UiRect::new(12., 12., 40., 40.)).unwrap();
    l.text(
        run,
        UiTextPlacement::new(UiRect::new(16., 16., 32., 32.)),
        effect,
    )
    .unwrap();
    l.pop_clip().unwrap();
    l.pop_clip().unwrap();
    l.solid_rect(UiRect::new(20., 28., 3., 8.), [0., 1., 0., 1.])
        .unwrap();
    l.finish().unwrap()
}
fn textures() -> BTreeMap<u64, SceneTexture> {
    let tex = |width, height, bytes: Vec<u8>| {
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width,
            height,
            mips: 1,
            bytes: bytes.into(),
        }
        .with_sampler(TextureSampler {
            filter: TextureFilter::Point,
            wrap_u: TextureWrap::Clamp,
            wrap_v: TextureWrap::Clamp,
        })
    };
    BTreeMap::from([
        (1, tex(4, 4, [0, 0, 255, 255].repeat(16))),
        (
            2,
            tex(
                2,
                2,
                vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
            ),
        ),
    ])
}
fn replace(run: &mut WindowlessRetained<'_>, p: &UiPresentation) {
    run.host_mut()
        .apply(RetainedUpdate::Ui(std::sync::Arc::new(
            UiOverlay::from_presentation(
                p,
                &textures(),
                &worldrunner_world_simulation::ui::presentation::UiLayers::default(),
            )
            .unwrap(),
        )))
        .unwrap();
}

#[test]
fn panel_frame_image_outline_golden() {
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let mut text = TextRun::new(fonts.register(crate::retained::ui_font::font()).unwrap());
    text.text = "AA".into();
    text.font_size = 20.;
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver, None).unwrap();
    let baseline = capture(&mut run, &events).1;
    let outlined = panel(
        &fonts,
        &text,
        UiTextEffect::Outline {
            color: [1., 0., 0., 1.],
            size: 2.,
        },
    );
    replace(&mut run, &outlined);
    let image = capture(&mut run, &events).1;
    assert_eq!(pixel(&image, 5, 32), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 45, 50), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 50, 50), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 18, 35), [255; 4]);
    assert_eq!(pixel(&image, 14, 35), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 12, 35), [0, 0, 0, 255]);
    assert_eq!(pixel(&image, 21, 33), [0, 255, 0, 255]);
    assert_eq!(pixel(&image, 2, 2), pixel(&baseline, 2, 2));
    for size in [5., 8.] {
        replace(
            &mut run,
            &panel(
                &fonts,
                &text,
                UiTextEffect::Glow {
                    color: [0., 1., 0., 1.],
                    size,
                },
            ),
        );
        let glow = capture(&mut run, &events).1;
        assert_eq!(pixel(&glow, 18, 35), [255; 4]);
        assert!(pixel(&glow, 14, 35)[1] > pixel(&glow, 12, 35)[1]);
        assert!(pixel(&glow, 12, 35)[1] > 0);
        assert_eq!(pixel(&glow, 11, 35), [0, 0, 0, 255], "nested clip");
    }
    replace(&mut run, &outlined);
    assert_eq!(capture(&mut run, &events).1, image);
    replace(&mut run, &UiPresentation::default());
    assert_eq!(capture(&mut run, &events).1, baseline);
    run.resize([48, 48]).unwrap();
    replace(&mut run, &outlined);
    let resized = capture(&mut run, &events).1;
    assert_eq!(pixel(&resized, 18, 35), [255; 4]);
    run.shutdown().unwrap();
}
