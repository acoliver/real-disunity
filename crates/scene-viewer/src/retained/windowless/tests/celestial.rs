use super::*;

fn scene(taa: bool, meshes: bool) -> RenderScene {
    let base = crate::renderer::sdsm_tests::sky_ssr_source(taa);
    let mut atmosphere = base.effects().unwrap().atmosphere().unwrap().clone();
    if meshes {
        atmosphere.meshes.push(SceneCelestialMesh {
            lighting: SceneCelestialLighting::NEUTRAL,
            tint: SceneTint::WHITE,
            anchor_millimeters: None,
            direction: SceneDirection::new([0, 1_000_000, 0]).unwrap(),
            angular_radius_millidegrees: 30_000,
            distance_millimeters: 100_000,
            model_radius_millimeters: 1000,
            rotation: SceneRotation::IDENTITY,
            mesh: MeshHandle::from_raw(0),
            material: MaterialHandle::from_raw(0),
        });
    }
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_atmosphere(atmosphere)
        .unwrap();
    base.with_objects_and_effects(base.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

fn configured_host(taa: bool, meshes: bool) -> RetainedHost {
    RetainedHost::new(
        scene(taa, meshes),
        crate::ViewerOptions {
            size: [8, 8],
            shadows: Some(crate::renderer::retained_tests::shadow_settings()),
            render_clock: RenderClock::Fixed(Duration::ZERO),
            frame_profile: crate::FrameProfile::Hdr {
                samples: 1,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        },
    )
    .unwrap()
}

#[test]
fn celestial_ssr_same_frame_capture_returns_receipt_with_and_without_taa() {
    for taa in [false, true] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut run = configured_host(taa, true).windowless(&mut driver).unwrap();
        let (_, image) = capture(&mut run, &events);
        assert_eq!(
            image.viewport,
            worldrunner_render_scene::Viewport {
                width: 8,
                height: 8
            }
        );
        assert!(events.borrow().iter().any(|event| matches!(
            event,
            RetainedEvent::Capture {
                generation: 0,
                outcome: RetainedCaptureOutcome::Completed {
                    frame: 0,
                    time: Duration::ZERO,
                    ..
                },
                ..
            }
        )));
        run.resize([6, 4]).unwrap();
        let (_, resized) = capture(&mut run, &events);
        assert_eq!(resized.viewport.width, 6);
        assert_eq!(resized.viewport.height, 4);
        run.shutdown().unwrap();
    }
}

fn constant_sky_scene() -> RenderScene {
    let base = crate::tests::scene(false);
    let effects = base
        .effects()
        .unwrap()
        .clone()
        .with_depth_fitting(SceneDepthFitting {
            near_step_mm: 100,
            far_step_mm: 1000,
            minimum_span_mm: 1000,
            minimum_span_policy: SceneDepthMinimumSpan::ExtendNear,
            active_maximum_mm: None,
            splits: SceneDepthSplits::OverlapRecurrence {
                multiplier_micro: 2_000_000,
                overlap_micro: 200_000,
            },
            adoption: SceneDepthAdoption::QuantizedPerBound {
                latency_frames: 2,
                contraction_delay_ms: 100,
            },
        })
        .unwrap()
        .with_ssr(SceneSsr {
            enabled: true,
            ..Default::default()
        })
        .unwrap()
        .with_atmosphere(SceneAtmosphere {
            celestial_composition: SceneCelestialComposition::Inherit,
            celestial_exposure_micro: None,
            background_depth_policy: SceneBackgroundDepthPolicy::FarPlane,
            scattering: None,
            horizon_radiance_micro: [0, 2_000_000, 0],
            zenith_radiance_micro: [0, 2_000_000, 0],
            gradient_exponent_micro: 1_000_000,
            sun_angular_radius_millidegrees: 0,
            fog: None,
            discs: vec![],
            meshes: vec![],
            background_distance_millimeters: 1_000_000,
        })
        .unwrap();
    base.with_objects_and_effects(vec![], Some(effects))
        .unwrap()
}

#[test]
fn celestial_free_same_frame_capture_is_byte_identical() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let host = RetainedHost::new(
        constant_sky_scene(),
        crate::ViewerOptions {
            size: [8, 8],
            shadows: Some(crate::renderer::retained_tests::shadow_settings()),
            render_clock: RenderClock::Fixed(Duration::ZERO),
            frame_profile: crate::FrameProfile::Hdr {
                samples: 1,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        },
    )
    .unwrap();
    let mut run = host.windowless(&mut driver).unwrap();
    let first = capture(&mut run, &events);
    // Empty geometry leaves constant radiance 2; Reinhard maps it to 2/3.
    // Compute sRGB bytes independently, away from a quantization boundary.
    let encoded = worldrunner_render_gpu::tolerance_test_support::encoded_steps(2.0 / 3.0, true);
    assert_eq!(first.0, Duration::ZERO);
    assert_eq!(first.1.viewport.width, 8);
    assert_eq!(first.1.viewport.height, 8);
    assert_eq!(first.1.pixels.len(), 8 * 8 * 4);
    // The HDR target stores half before tone mapping and the sRGB attachment quantises: the
    // display code is within the shared output bound plus one half intermediate of the
    // unquantised code, on any driver.
    for pixel in first.1.pixels.chunks_exact(4) {
        assert_eq!([pixel[0], pixel[2], pixel[3]], [0, 0, 255]);
        worldrunner_render_gpu::tolerance_test_support::assert_code_within(
            pixel[1],
            encoded,
            worldrunner_render_gpu::tolerance_test_support::HALF_INTERMEDIATE_STEPS,
            "constant sky green",
        );
    }
    // The same code on the same device repeats exactly.
    assert_eq!(first, capture(&mut run, &events));
    run.shutdown().unwrap();
}
