mod annotation_depth;
mod annotations;
mod blend_tests;
mod graphics;
mod text_flow;
fn geometry_pixel(image: &RgbaImage) -> [u32; 2] {
    let width = image.viewport.width;
    let height = image.viewport.height;
    image
        .pixels
        .chunks_exact(4)
        .enumerate()
        .filter(|(_, pixel)| pixel[..3] != [0, 0, 0])
        .map(|(i, _)| {
            let i = u32::try_from(i).unwrap();
            [i % width, height - 1 - i / width]
        })
        .min_by_key(|[x, y]| x.abs_diff(width / 2) + y.abs_diff(height / 2))
        .expect("the shadowed scene must draw geometry over its black background")
}

#[test]
fn overlay_covers_retained_geometry_with_straight_alpha_in_ldr_and_msaa_hdr() {
    for profile in [
        crate::FrameProfile::Legacy,
        crate::FrameProfile::Hdr {
            samples: 4,
            tone_map: crate::ToneMap::Reinhard,
        },
    ] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut host = overlay_host();
        host.options.frame_profile = profile;
        let mut run = host.windowless(&mut driver).unwrap();
        let baseline = capture(&mut run, &events).1;
        let [x, y] = geometry_pixel(&baseline);
        let mut ui = plan([1., 0., 0., 1.]);
        let offset = glam::UVec2::new(x, y).as_vec2();
        for corner in &mut ui.texts[0].quads[0].corners {
            corner[0] += offset.x - 16.;
            corner[1] += offset.y - 16.;
        }
        std::sync::Arc::make_mut(&mut ui.texts[0].texture.as_mut().unwrap().bytes).fill(255);
        apply(&mut run, ui.clone());
        assert_eq!(pixel(&capture(&mut run, &events).1, x, y), [255, 0, 0, 255]);
        let mut front = ui.texts[0].clone();
        front.color = [0., 1., 0., 0.5];
        front.sorting_order = 1;
        ui.texts.push(front);
        apply(&mut run, ui);
        assert_blended_within_one(
            pixel(&capture(&mut run, &events).1, x, y),
            [188, 188, 0, 255],
        );
        run.shutdown().unwrap();
    }
}

use super::*;
use worldrunner_render_plan::ui::{UiGlyphQuad, UiRenderPlan, UiTextContribution};
use worldrunner_render_scene::simulation::UiOverlay;
use worldrunner_world_simulation::{ObjectDesc, World, WorldSettings};

fn plan(color: [f32; 4]) -> UiRenderPlan {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    UiRenderPlan {
        layout: world.ui_layout([64, 64].into()).unwrap(),
        graphics: vec![],
        texts: vec![UiTextContribution {
            object,
            canvas: object,
            hierarchy_order: 0,
            clip_rect: [0, 0, 64, 64],
            overlay: true,
            sorting_layer: 0,
            sorting_order: 0,
            color,
            texture: Some(
                worldrunner_world_simulation::presentation::CoverageTexture {
                    width: 2,
                    height: 2,
                    bytes: vec![255, 0, 0, 255].into(),
                },
            ),
            quads: vec![UiGlyphQuad {
                color: [1.; 4],
                corners: [[8., 8., 0.], [24., 8., 0.], [24., 24., 0.], [8., 24., 0.]],
                uv: [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
            }],
        }],
    }
}
fn apply(run: &mut WindowlessRetained<'_>, plan: UiRenderPlan) {
    run.host_mut()
        .apply(RetainedUpdate::Ui(std::sync::Arc::new(
            UiOverlay::new(plan).unwrap(),
        )))
        .unwrap();
}
fn pixel(image: &RgbaImage, x: u32, y: u32) -> &[u8] {
    let index = ((image.viewport.height - 1 - y) * image.viewport.width + x) as usize * 4;
    &image.pixels[index..index + 4]
}
fn assert_blended_within_one(actual: &[u8], expected: [u8; 4]) {
    assert_eq!(actual.len(), expected.len());
    // Metal and lavapipe can round straight-alpha blending one byte apart.
    for (channel, (&actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            actual.abs_diff(expected) <= 1,
            "blended channel {channel}: {actual} versus {expected}"
        );
    }
}
fn region(image: &RgbaImage, expected: [u8; 4]) {
    for y in 20..24 {
        for x in 8..12 {
            assert_eq!(pixel(image, x, y), expected, "pixel {x},{y}");
        }
    }
}
fn overlay_host() -> RetainedHost {
    let mut host = host(crate::renderer::retained_tests::terrain_source());
    host.options.shadows = Some(crate::renderer::retained_tests::shadow_settings());
    host.options.frame_profile = crate::FrameProfile::Hdr {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
    };
    host
}
fn assert_statistics(run: &WindowlessRetained<'_>, generation: u64, instances: u64) {
    let stats = run.host().statistics().unwrap();
    assert_eq!(stats.generation, generation);
    assert_eq!(stats.scene.passes.ui.instances, instances);
    assert_eq!(stats.scene.passes.ui.calls, instances);
    assert_eq!(stats.scene.textures.uploaded_bytes, 0);
    assert_eq!(stats.scene.mesh_buffers.uploaded_bytes, 0);
    assert_eq!(
        stats.scene.effects.shadows.installation,
        crate::PassInstallation::Installed
    );
}

#[test]
fn retained_overlay_pixels_follow_generations_after_tone_mapping_and_resize() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events).1;
    apply(&mut run, plan([1., 0., 0., 1.]));
    let image = capture(&mut run, &events).1;
    region(&image, [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 21, 10), [255, 0, 0, 255]);
    assert_eq!(pixel(&image, 10, 10), pixel(&baseline, 10, 10));
    assert_eq!(pixel(&image, 30, 30), pixel(&baseline, 30, 30));
    assert_statistics(&run, 1, 1);
    run.resize([48, 32]).unwrap();
    region(&capture(&mut run, &events).1, [255, 0, 0, 255]);
    apply(&mut run, plan([0., 1., 0., 1.]));
    region(&capture(&mut run, &events).1, [0, 255, 0, 255]);
    assert_statistics(&run, 2, 1);
    assert!(events.borrow().iter().any(|event| matches!(event,
        RetainedEvent::Capture { generation: 1, outcome: RetainedCaptureOutcome::Completed { observation, .. }, .. }
        if observation.generation == 1 && observation.scene.passes.ui.instances == 1)));
    run.shutdown().unwrap();
}

#[test]
fn overlay_sorting_exposure_camera_replacement_and_clear_preserve_pixel_space() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    capture(&mut run, &events);
    let mut ui = plan([1., 0., 0., 1.]);
    let mut front = ui.texts[0].clone();
    front.color = [0., 1., 0., 1.];
    front.sorting_layer = 2;
    front.sorting_order = -5;
    ui.texts.insert(0, front);
    apply(&mut run, ui.clone());
    region(&capture(&mut run, &events).1, [0, 255, 0, 255]);
    ui.texts[1].sorting_layer = 2;
    ui.texts[1].sorting_order = -4;
    apply(&mut run, ui);
    let first = capture(&mut run, &events).1;
    region(&first, [255, 0, 0, 255]);
    change_camera_and_exposure(&mut run);
    let changed = capture(&mut run, &events).1;
    region(&changed, [255, 0, 0, 255]);
    assert_ne!(first, changed);
    let scene = crate::renderer::retained_tests::terrain_source();
    run.host_mut()
        .apply(RetainedUpdate::Resources {
            resources: scene.resources().clone(),
            meshes: scene.geometry().meshes().to_vec(),
        })
        .unwrap();
    region(&capture(&mut run, &events).1, [255, 0, 0, 255]);
    let mut empty = plan([1.; 4]);
    empty.texts.clear();
    apply(&mut run, empty);
    let cleared = capture(&mut run, &events).1;
    assert_ne!(pixel(&cleared, 10, 21), [255, 0, 0, 255]);
    assert_eq!(
        run.host().statistics().unwrap().scene.passes.ui,
        crate::DrawCount::default()
    );
    run.shutdown().unwrap();
}
fn change_camera_and_exposure(run: &mut WindowlessRetained<'_>) {
    let options = run.host().options().clone();
    let mut pose = options.pose;
    pose.eye_millimeters[0] += 1000;
    run.host_mut()
        .apply(RetainedUpdate::Camera {
            pose,
            lens: options.camera,
            convention: options.camera_convention,
        })
        .unwrap();
    let source = crate::renderer::retained_tests::terrain_source();
    let base = source.effects().unwrap();
    let mut grading = *base.grading();
    grading.exposure_micro = 4_000_000;
    let effects = SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *base.sky(),
        sun: *base.sun(),
        ambient: *base.ambient(),
        shadows: base.shadows().clone(),
        ssao: None,
        camera: *base.camera(),
        grading,
        post: vec![],
        anti_alias: *base.anti_alias(),
    })
    .unwrap();
    run.host_mut()
        .apply(RetainedUpdate::Settings(Box::new(effects)))
        .unwrap();
}

#[test]
fn shutting_down_pending_overlay_capture_publishes_only_cancellation() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    apply(&mut run, plan([1.; 4]));
    let id = run.host_mut().request_capture(1).unwrap();
    assert!(run.frame().unwrap());
    run.shutdown().unwrap();
    let events = events.borrow();
    let outcomes: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            RetainedEvent::Capture {
                id: actual,
                outcome,
                ..
            } if *actual == id => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(outcomes, [&RetainedCaptureOutcome::Cancelled]);
}

#[test]
fn overlay_rejects_world_canvas_missing_coverage_and_invalid_geometry() {
    let mut ui = plan([1.; 4]);
    ui.texts[0].overlay = false;
    assert!(UiOverlay::new(ui).is_err());
    let mut ui = plan([1.; 4]);
    ui.texts[0].texture = None;
    assert!(UiOverlay::new(ui).is_err());
    let mut ui = plan([1.; 4]);
    ui.texts[0].texture.as_mut().unwrap().width = 3;
    assert!(UiOverlay::new(ui).is_err());
    let mut ui = plan([1.; 4]);
    ui.texts[0].quads[0].corners[0][0] = f32::NAN;
    assert!(UiOverlay::new(ui).is_err());
}
