use super::*;
use worldrunner_render_plan::{SceneGeometry, SceneMesh, SceneMeshLod, SceneParts, SceneVertex};
use worldrunner_world_simulation::ui::{
    FontBook, UiLimits,
    annotations::{
        Annotation, AnnotationDepth, AnnotationPoint, AnnotationPrimitive, extract_annotations,
    },
};

fn box_scene() -> RenderScene {
    let base = crate::renderer::retained_tests::source();
    let vertices = [
        [-1000, -500, -1000],
        [1000, -500, -1000],
        [1000, -500, 1000],
        [-1000, -500, 1000],
        [-1000, 500, -1000],
        [1000, 500, -1000],
        [1000, 500, 1000],
        [-1000, 500, 1000],
    ]
    .map(|position_millimeters| SceneVertex {
        position_millimeters,
        normal_micro: [0, -1_000_000, 0],
        texcoord_micro: [0; 2],
        tangent_micro: None,
    })
    .to_vec();
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vertices,
            vec![
                0, 1, 2, 0, 2, 3, 5, 4, 7, 5, 7, 6, 4, 0, 3, 4, 3, 7, 1, 5, 6, 1, 6, 2, 3, 2, 6, 3,
                6, 7, 4, 5, 1, 4, 1, 0,
            ],
            None,
        )
        .unwrap(),
    ])
    .unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new(vec![mesh], base.geometry().objects().to_vec()).unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}
fn depth_host(profile: crate::FrameProfile) -> RetainedHost {
    let mut options = crate::tests::options();
    options.size = [64, 64];
    options.render_clock = RenderClock::Fixed(Duration::ZERO);
    options.frame_profile = profile;
    options.pose.eye_millimeters = [0, -5000, 0];
    RetainedHost::new(box_scene(), options).unwrap()
}
fn line(a: glam::Vec3, b: glam::Vec3, depth: AnnotationDepth) -> Annotation {
    Annotation {
        primitive: AnnotationPrimitive::Line {
            from: AnnotationPoint::World(a),
            to: AnnotationPoint::World(b),
            width: 6.,
            colour: [1., 0., 0., 1.],
        },
        draw_order: (0, 0, 0),
        clip_rect: None,
        depth,
    }
}
fn screen_line() -> Annotation {
    Annotation {
        primitive: AnnotationPrimitive::Line {
            from: AnnotationPoint::Screen(glam::Vec2::new(10., 32.)),
            to: AnnotationPoint::Screen(glam::Vec2::new(54., 32.)),
            width: 6.,
            colour: [0., 1., 0., 1.],
        },
        draw_order: (0, 0, 0),
        clip_rect: None,
        depth: AnnotationDepth::AlwaysVisible,
    }
}
fn submit(run: &mut WindowlessRetained<'_>, annotation: Annotation) {
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let plan =
        extract_annotations(&[annotation], &fonts, run.host().presented_view().unwrap()).unwrap();
    assert!(plan.draws.iter().all(|draw| draw.depth_test));
    run.host_mut()
        .apply(RetainedUpdate::Ui(std::sync::Arc::new(
            UiOverlay::from_presentation(&plan, &std::collections::BTreeMap::new()).unwrap(),
        )))
        .unwrap();
}
fn profiles() -> [crate::FrameProfile; 4] {
    [
        crate::FrameProfile::Legacy,
        crate::FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        crate::FrameProfile::Hdr {
            samples: 4,
            tone_map: crate::ToneMap::Reinhard,
        },
        crate::FrameProfile::HdrUpscaled {
            samples: 4,
            tone_map: crate::ToneMap::Reinhard,
            scale: worldrunner_render_plan::post::RenderScale::new(500_000).unwrap(),
            upscale: worldrunner_render_plan::post::UpscaleSettings {
                mode: worldrunner_render_plan::post::UpscaleMode::Bilinear,
                sharpness: None,
            },
        },
    ]
}
#[test]
fn depth_tested_annotation_line_is_partially_occluded_by_box() {
    for profile in profiles() {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut run = depth_host(profile).windowless(&mut driver).unwrap();
        let baseline = capture(&mut run, &events).1;
        let a = glam::Vec3::new(-1.8, -1.5, 0.);
        let b = glam::Vec3::new(1.8, 1.5, 0.);
        submit(&mut run, line(a, b, AnnotationDepth::DepthTested));
        let image = capture(&mut run, &events).1;
        assert_eq!(pixel(&image, 20, 32), [255, 0, 0, 255]);
        assert_ne!(pixel(&baseline, 36, 32), [0, 0, 0, 255]);
        assert_eq!(pixel(&image, 36, 32), pixel(&baseline, 36, 32));
        assert_depth_statistics(&run, &events);
        let fonts = FontBook::new(UiLimits::default()).unwrap();
        let always = extract_annotations(
            &[line(a, b, AnnotationDepth::AlwaysVisible)],
            &fonts,
            run.host().presented_view().unwrap(),
        )
        .unwrap();
        run.host_mut()
            .apply(RetainedUpdate::Ui(std::sync::Arc::new(
                UiOverlay::from_presentation(&always, &std::collections::BTreeMap::new()).unwrap(),
            )))
            .unwrap();
        assert_eq!(
            pixel(&capture(&mut run, &events).1, 36, 32),
            [255, 0, 0, 255]
        );
        assert_eq!(
            run.host()
                .statistics()
                .unwrap()
                .scene
                .passes
                .ui_depth_tested
                .instances,
            0
        );
        run.shutdown().unwrap();
    }
}
#[test]
fn depth_tested_surface_annotation_uses_one_millimetre_bias() {
    for profile in profiles() {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut run = depth_host(profile).windowless(&mut driver).unwrap();
        let baseline = capture(&mut run, &events).1;
        for y in [-0.5, -0.4995] {
            submit(
                &mut run,
                line(
                    glam::Vec3::new(-0.8, y, 0.),
                    glam::Vec3::new(0.8, y, 0.),
                    AnnotationDepth::DepthTested,
                ),
            );
            assert_eq!(
                pixel(&capture(&mut run, &events).1, 32, 32),
                [255, 0, 0, 255]
            );
        }
        submit(
            &mut run,
            line(
                glam::Vec3::new(-0.8, -0.498, 0.),
                glam::Vec3::new(0.8, -0.498, 0.),
                AnnotationDepth::DepthTested,
            ),
        );
        assert_eq!(
            pixel(&capture(&mut run, &events).1, 32, 32),
            pixel(&baseline, 32, 32)
        );
        run.shutdown().unwrap();
    }
}

fn assert_depth_statistics(run: &WindowlessRetained<'_>, events: &RefCell<Vec<RetainedEvent>>) {
    let expected = crate::DrawCount {
        calls: 2,
        instances: 2,
    };
    let stats = run.host().statistics().unwrap().scene.passes;
    assert_eq!(stats.ui_depth_tested, expected);
    assert_eq!(stats.ui, expected);
    let generation = run.host().accepted_generation();
    assert!(events.borrow().iter().any(|event| matches!(event,
        RetainedEvent::Capture { generation: actual,
            outcome: RetainedCaptureOutcome::Completed { observation, .. }, .. }
        if *actual == generation && observation.scene.passes.ui_depth_tested == expected
            && observation.scene.passes.ui == expected)));
}
#[test]
fn depth_tested_annotation_tracks_presented_camera_and_resized_depth() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = depth_host(profiles()[3]).windowless(&mut driver).unwrap();
    for size in [[64, 64], [96, 80]] {
        run.resize(size).unwrap();
        for eye in [-5000, -6000] {
            let options = run.host().options().clone();
            let mut pose = options.pose;
            pose.eye_millimeters[1] = eye;
            run.host_mut()
                .apply(RetainedUpdate::Camera {
                    pose,
                    lens: options.camera,
                    convention: options.camera_convention,
                })
                .unwrap();
            let empty = UiOverlay::from_presentation(
                &worldrunner_render_plan::ui::UiPresentation::default(),
                &std::collections::BTreeMap::new(),
            )
            .unwrap();
            run.host_mut()
                .apply(RetainedUpdate::Ui(std::sync::Arc::new(empty)))
                .unwrap();
            check_presented_line(&mut run, &events);
        }
    }
    run.shutdown().unwrap();
}
fn check_presented_line(run: &mut WindowlessRetained<'_>, events: &RefCell<Vec<RetainedEvent>>) {
    let baseline = capture(run, events).1;
    let a = glam::Vec3::new(-1.8, -1.5, 0.);
    let b = glam::Vec3::new(1.8, 1.5, 0.);
    let view = *run.host().presented_view().unwrap();
    let pixel_at = |point| {
        let p = view.world_to_screen(point).unwrap().truncate().as_uvec2();
        [p.x, view.viewport().height - 1 - p.y]
    };
    let [front_x, front_y] = pixel_at(a.lerp(b, 0.2));
    let [back_x, back_y] = pixel_at(glam::Vec3::ZERO);
    submit(run, line(a, b, AnnotationDepth::DepthTested));
    let image = capture(run, events).1;
    assert_eq!(pixel(&image, front_x, front_y), [255, 0, 0, 255]);
    assert_eq!(
        pixel(&image, back_x, back_y),
        pixel(&baseline, back_x, back_y)
    );
    assert_depth_statistics(run, events);
}

#[test]
fn depth_tested_annotation_preserves_shadowed_capture_continuation() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut host = depth_host(profiles()[2]);
    host.options.shadows = Some(crate::renderer::retained_tests::shadow_settings());
    let mut run = host.windowless(&mut driver).unwrap();
    check_presented_line(&mut run, &events);
    assert_eq!(
        run.host()
            .statistics()
            .unwrap()
            .scene
            .effects
            .shadows
            .installation,
        crate::PassInstallation::Installed
    );
    run.shutdown().unwrap();
}

#[test]
fn depth_tested_and_always_visible_annotations_share_painter_order() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = depth_host(profiles()[2]).windowless(&mut driver).unwrap();
    capture(&mut run, &events);
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    for (tested_order, expected) in [(1, [255, 0, 0, 255]), (-1, [0, 255, 0, 255])] {
        let mut tested = line(
            glam::Vec3::new(-0.8, -0.6, 0.),
            glam::Vec3::new(0.8, -0.6, 0.),
            AnnotationDepth::DepthTested,
        );
        tested.draw_order.0 = tested_order;
        let mut hidden = line(
            glam::Vec3::new(-0.8, 0.6, 0.),
            glam::Vec3::new(0.8, 0.6, 0.),
            AnnotationDepth::DepthTested,
        );
        hidden.draw_order.0 = 2;
        let always = screen_line();
        let plan = extract_annotations(
            &[hidden, always, tested],
            &fonts,
            run.host().presented_view().unwrap(),
        )
        .unwrap();
        run.host_mut()
            .apply(RetainedUpdate::Ui(std::sync::Arc::new(
                UiOverlay::from_presentation(&plan, &std::collections::BTreeMap::new()).unwrap(),
            )))
            .unwrap();
        assert_eq!(pixel(&capture(&mut run, &events).1, 32, 32), expected);
        let stats = run.host().statistics().unwrap().scene.passes;
        assert_eq!(stats.ui.instances, 6);
        assert_eq!(stats.ui_depth_tested.instances, 4);
    }
    run.shutdown().unwrap();
}
