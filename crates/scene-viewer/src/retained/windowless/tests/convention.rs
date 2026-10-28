mod effects;
mod normal_map;

use super::*;
use crate::{CaptureJitter, CaptureOutput, RetainedCaptureProducts, RetainedCaptureRecipe};
use worldrunner_camera::{ViewConvention, ViewHandedness};

fn convention(handedness: ViewHandedness) -> ViewConvention {
    ViewConvention { handedness }
}

fn scene() -> RenderScene {
    let base = basis_scene();
    let initial = base.effects().unwrap();
    let effects = SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *initial.sky(),
        sun: *initial.sun(),
        ambient: *initial.ambient(),
        shadows: SceneShadowCascades::camera_splits(vec![2000, 4000, 8000], 100_000).unwrap(),
        camera: *initial.camera(),
        grading: *initial.grading(),
        post: vec![],
        anti_alias: *initial.anti_alias(),
        ssao: Some(SceneSsao {
            depth_footprint: SceneSsaoDepthFootprint::default(),
            kernel: SceneSsaoKernel::Hemisphere,
            filter: SceneSsaoFilter::DepthAwareBox,
            sample_count: 4,
            bias_millimeters: 1,
            radius_millimeters: 100,
            intensity_micro: 1_000_000,
            horizon_angle_millidegrees: 90_000,
        }),
    })
    .unwrap()
    .with_ssr(SceneSsr {
        enabled: true,
        max_steps: 8,
        ..Default::default()
    })
    .unwrap();
    let object = base.geometry().objects()[0];
    base.with_objects_and_effects(
        vec![SceneObject::new(
            SceneTransform {
                position_millimeters: [-400, 0, 0],
                scale_millionths: 400_000,
                ..*object.transform()
            },
            object.mesh(),
            object.material(),
        )],
        Some(effects),
    )
    .unwrap()
}

fn recipe() -> RetainedCaptureRecipe {
    RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![
            CaptureOutput::Srgb8,
            CaptureOutput::LinearHdr,
            CaptureOutput::Depth,
            CaptureOutput::Normals,
            CaptureOutput::AmbientOcclusion,
            CaptureOutput::SsrConfidence,
        ],
        4 * 1024 * 1024,
    )
    .unwrap()
}

fn products(
    run: &mut WindowlessRetained<'_>,
    events: &RefCell<Vec<RetainedEvent>>,
) -> RetainedCaptureProducts {
    let generation = run.host().accepted_generation();
    let id = run
        .host_mut()
        .request_capture_recipe(generation, recipe())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        run.frame().unwrap();
        run.poll().unwrap();
        if let Some(products) = events.borrow().iter().find_map(|event| match event {
            RetainedEvent::Capture {
                id: actual,
                generation: actual_generation,
                outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
            } if *actual == id => {
                assert_eq!(*actual_generation, generation);
                Some(products.clone())
            }
            _ => None,
        }) {
            return products;
        }
        assert!(Instant::now() < deadline, "camera capture timed out");
        std::thread::yield_now();
    }
}

fn set_camera(run: &mut WindowlessRetained<'_>, handedness: ViewHandedness) {
    let options = crate::tests::options();
    run.host_mut()
        .apply(RetainedUpdate::Camera {
            pose: options.pose,
            lens: options.camera,
            convention: convention(handedness),
        })
        .unwrap();
}

fn assert_geometry_mirrored(right: &RetainedCaptureProducts, left: &RetainedCaptureProducts) {
    let right_depth = right.depth.as_ref().unwrap();
    let left_depth = left.depth.as_ref().unwrap();
    let width = usize::try_from(right_depth.viewport.width).unwrap();
    let height = usize::try_from(right_depth.viewport.height).unwrap();
    let normals = left.normals.as_ref().unwrap();
    let mut covered = 0;
    for y in 0..height {
        for x in 0..width {
            let depth = right_depth.pixels[y * width + x];
            let reflected = y * width + width - 1 - x;
            assert!((depth - left_depth.pixels[reflected]).abs() < 0.00001);
            if normals.pixels[reflected][3] > 0.0 {
                covered += 1;
                assert!(normals.pixels[reflected][2] > 0.0);
                let reference = right.normals.as_ref().unwrap().pixels[y * width + x];
                let actual = normals.pixels[reflected];
                for channel in 0..4 {
                    assert!(
                        (reference[channel] - actual[channel]).abs() < 0.002,
                        "normal mismatch at {x},{y}: {reference:?} versus {actual:?}"
                    );
                }
            }
        }
    }
    assert!(covered > width);
    for products in [right, left] {
        let image = products.srgb8.as_ref().unwrap();
        let background = &image.pixels[..4];
        assert!(
            image
                .pixels
                .chunks_exact(4)
                .filter(|pixel| *pixel != background)
                .count()
                > width
        );
    }
}

#[test]
fn retained_camera_conventions_preserve_culling_metadata_shadows_resize_and_fixed_capture() {
    for shadows in [
        None,
        Some(crate::renderer::retained_tests::shadow_settings()),
    ] {
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let host = camera_host(shadows);
        let mut run = host.windowless(&mut driver).unwrap();
        for size in [[64, 64], [48, 32]] {
            run.resize(size).unwrap();
            set_camera(&mut run, ViewHandedness::RightHanded);
            let right = products(&mut run, &events);
            set_camera(&mut run, ViewHandedness::DirectXLeftHanded);
            let left = products(&mut run, &events);
            assert_geometry_mirrored(&right, &left);
            assert_eq!(left, products(&mut run, &events));
            assert_eq!(
                run.host()
                    .view()
                    .unwrap()
                    .unwrap()
                    .resolved_view()
                    .convention(),
                convention(ViewHandedness::DirectXLeftHanded)
            );
            assert_eq!(
                run.host().ready_generation(),
                run.host().accepted_generation()
            );
        }
        run.shutdown().unwrap();
    }
}

#[test]
fn both_camera_conventions_cull_mesh_and_terrain_undersides() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let host = camera_host(None);
    let mut run = host.windowless(&mut driver).unwrap();
    for handedness in [
        ViewHandedness::RightHanded,
        ViewHandedness::DirectXLeftHanded,
    ] {
        run.host_mut()
            .apply(RetainedUpdate::Camera {
                pose: worldrunner_render_scene::CameraPose {
                    eye_millimeters: [0, -3000, -2000],
                    target_millimeters: [0; 3],
                },
                lens: crate::tests::options().camera,
                convention: convention(handedness),
            })
            .unwrap();
        assert!(
            products(&mut run, &events)
                .normals
                .unwrap()
                .pixels
                .iter()
                .all(|normal| normal[3] == 0.0)
        );
    }
    run.shutdown().unwrap();
}

fn camera_host(
    shadows: Option<worldrunner_render_scene::shadows::DirectionalShadowSettings>,
) -> RetainedHost {
    RetainedHost::new(
        scene(),
        crate::ViewerOptions {
            shadows,
            size: [64, 64],
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

fn basis_scene() -> RenderScene {
    let base = crate::renderer::retained_tests::terrain_source();
    let lod = &base.geometry().meshes()[0].lods()[0];
    let vertices = lod
        .vertices()
        .iter()
        .copied()
        .map(|mut vertex| {
            vertex.tangent_micro = Some([1_000_000, 0, 0, 1_000_000]);
            vertex.texcoord_micro = [
                (vertex.position_millimeters[0] + 1000) * 500,
                (vertex.position_millimeters[1] + 1000) * 500,
            ];
            vertex
        })
        .collect();
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, lod.indices().to_vec(), None).unwrap(),
    ])
    .unwrap();
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: SceneGeometry::new(vec![mesh], base.geometry().objects().to_vec()).unwrap(),
        terrain: base.terrain().cloned(),
        effects: base.effects().cloned(),
    })
    .unwrap()
}
