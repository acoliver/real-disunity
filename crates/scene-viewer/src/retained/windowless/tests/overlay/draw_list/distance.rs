//! Distance masks through the retained compositor. These tests require a GPU.
use super::*;
use worldrunner_world_simulation::presentation::CoverageTexture;
use worldrunner_world_simulation::ui::UiTexture;
use worldrunner_world_simulation::ui::presentation::{UiDistanceMode, UiDistanceStyle};

fn panel() -> UiDraw {
    let mut p = draw(false);
    p.corners = [[4., 4., 0.], [60., 4., 0.], [60., 60., 0.], [4., 60., 0.]];
    p.texture = None;
    p.corner_colors = None;
    p.color = [0., 0., 0., 1.];
    p
}
fn glyph(mode: UiDistanceMode, radius: f32) -> UiDraw {
    let mut q = draw(false);
    q.kind = UiDrawKind::Text;
    q.corner_colors = None;
    q.texture = Some(UiTexture::Coverage(0));
    q.hierarchy_order = 1;
    q.distance_field = Some(UiDistanceStyle {
        spread_px: 25.5,
        radius_px: radius,
        mode,
        effect_color: if mode == UiDistanceMode::Outline {
            [1., 0., 0., 1.]
        } else {
            [0., 1., 0., 1.]
        },
    });
    q
}
fn publish(run: &mut WindowlessRetained<'_>, draws: Vec<UiDraw>, bytes: Vec<u8>) {
    let p = UiPresentation {
        draws,
        coverage: BTreeMap::from([(
            0,
            CoverageTexture {
                width: 16,
                height: 16,
                bytes: bytes.into(),
            },
        )]),
        ..Default::default()
    };
    run.host_mut()
        .apply(RetainedUpdate::Ui(std::sync::Arc::new(
            UiOverlay::from_presentation(
                &p,
                &BTreeMap::new(),
                &worldrunner_world_simulation::ui::presentation::UiLayers::default(),
            )
            .unwrap(),
        )))
        .unwrap();
}
fn stripes() -> Vec<u8> {
    (0..16)
        .flat_map(|_| {
            [
                0, 88, 103, 118, 125, 125, 130, 255, 255, 130, 125, 125, 118, 103, 88, 0,
            ]
        })
        .collect()
}

#[test]
fn outline_pixels() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver, None).unwrap();
    let baseline = capture(&mut run, &events).1;
    for depth in [false, true] {
        let mut q = glyph(UiDistanceMode::Outline, 2.);
        q.depth_test = depth;
        publish(&mut run, vec![panel(), q], stripes());
        let image = capture(&mut run, &events).1;
        for (x, expected) in [
            (8, [0, 0, 0, 255]),
            (10, [0, 0, 0, 255]),
            (12, [255, 0, 0, 255]),
            (13, [255, 0, 0, 255]),
            (14, [255; 4]),
            (15, [255; 4]),
        ] {
            assert_blended_within_one(pixel(&image, x, 16), expected);
        }
        assert_eq!(pixel(&image, 2, 2), pixel(&baseline, 2, 2));
    }
    run.shutdown().unwrap();
}

#[test]
fn glow_pixels() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver, None).unwrap();
    for (radius, expected, color) in [
        (2., [0, 237, 0, 255], [0., 1., 0., 1.]),
        (5., [0, 252, 0, 255], [0., 1., 0., 1.]),
        (8., [0, 0, 254, 255], [0., 0., 1., 1.]),
    ] {
        let mut q = glyph(UiDistanceMode::Glow, radius);
        q.distance_field.as_mut().unwrap().effect_color = color;
        publish(&mut run, vec![panel(), q], stripes());
        let image = capture(&mut run, &events).1;
        assert_blended_within_one(pixel(&image, 12, 16), expected);
        assert_blended_within_one(pixel(&image, 13, 16), expected);
        assert_eq!(pixel(&image, 8, 16), [0, 0, 0, 255]);
        assert_eq!(pixel(&image, 15, 16), [255; 4]);
        for (x, distance) in [(9, -7.9_f32), (10, -4.9), (11, -1.9)] {
            let t = ((distance + radius) / radius).clamp(0., 1.);
            let coverage = t * t * (3. - 2. * t);
            let expected = [
                byte(color[0] * coverage),
                byte(color[1] * coverage),
                byte(color[2] * coverage),
                255,
            ];
            assert_blended_within_one(pixel(&image, x, 16), expected);
        }
    }
    run.shutdown().unwrap();
}

#[test]
fn distance_rotation_stretch_alpha_and_mode_transitions_pixels() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver, None).unwrap();
    for (scale, angle, alpha, green) in [
        (glam::Vec2::ONE, 0., 1., 237),
        (glam::Vec2::splat(2.), 0., 1., 188),
        (glam::Vec2::ONE, std::f32::consts::FRAC_PI_4, 1., 237),
        (
            glam::Vec2::new(2., 1.),
            std::f32::consts::FRAC_PI_4,
            0.5,
            174,
        ),
    ] {
        let mut q = glyph(UiDistanceMode::Glow, 2.);
        q.color[3] = alpha;
        q.distance_field.as_mut().unwrap().effect_color[3] = alpha;
        let (sin, cos) = angle.sin_cos();
        q.corners = [[-8., -8.], [8., -8.], [8., 8.], [-8., 8.]].map(|p| {
            let p = glam::Vec2::from_array(p) * scale;
            [32. + cos * p.x - sin * p.y, 32. + sin * p.x + cos * p.y, 0.]
        });
        publish(&mut run, vec![panel(), q.clone()], vec![125; 256]);
        assert_blended_within_one(
            pixel(&capture(&mut run, &events).1, 32, 32),
            [0, green, 0, 255],
        );
        q.distance_field.as_mut().unwrap().mode = UiDistanceMode::Outline;
        q.distance_field.as_mut().unwrap().effect_color = [1., 0., 0., alpha];
        publish(&mut run, vec![panel(), q.clone()], vec![125; 256]);
        assert_blended_within_one(
            pixel(&capture(&mut run, &events).1, 32, 32),
            [
                if alpha.to_bits() == 1_f32.to_bits() {
                    255
                } else {
                    188
                },
                0,
                0,
                255,
            ],
        );
        transitions(&mut run, &events, q);
    }
    run.shutdown().unwrap();
}
fn transitions(
    run: &mut WindowlessRetained<'_>,
    events: &RefCell<Vec<RetainedEvent>>,
    mut q: UiDraw,
) {
    q.color = [1.; 4];
    q.distance_field.as_mut().unwrap().effect_color = [1., 0., 0., 1.];
    let mut plain = q.clone();
    plain.distance_field = None;
    plain.hierarchy_order = 2;
    publish(run, vec![panel(), q.clone(), plain.clone()], vec![125; 256]);
    // The final ordinary draw must sample 125/255 coverage, never a stale distance mask.
    assert_blended_within_one(pixel(&capture(run, events).1, 32, 32), [255, 186, 186, 255]);
    q.hierarchy_order = 2;
    plain.hierarchy_order = 1;
    let mut final_plain = plain.clone();
    final_plain.hierarchy_order = 3;
    publish(
        run,
        vec![panel(), plain, q.clone(), final_plain],
        vec![125; 256],
    );
    let image = capture(run, events).1;
    assert!(pixel(&image, 32, 32)[0] >= 186);
    assert_blended_within_one(pixel(&image, 32, 32), [255, 186, 186, 255]);
    let mut blue = q.clone();
    blue.hierarchy_order = 3;
    blue.distance_field.as_mut().unwrap().effect_color = [0., 0., 1., 1.];
    publish(run, vec![panel(), q, blue], vec![125; 256]);
    assert_eq!(pixel(&capture(run, events).1, 32, 32), [0, 0, 255, 255]);
}

fn byte(linear: f32) -> u8 {
    u8::try_from(glam::Vec2::splat(srgb(linear)).as_uvec2().x).unwrap()
}
