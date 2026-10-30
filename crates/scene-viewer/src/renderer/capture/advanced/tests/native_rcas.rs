use super::*;
use worldrunner_render_plan::*;
use worldrunner_render_scene::offscreen::RgbaImage;

fn assert_rcas(renderer: &Renderer, expected: Option<crate::NativeRcasSettings>) {
    assert_eq!(
        renderer
            .presentation
            .scheduler
            .configuration()
            .post
            .native_rcas,
        expected
    );
}

fn ordinary_capture(host: &mut RetainedHost, renderer: &mut Renderer) -> RgbaImage {
    let id = host.request_capture(0).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        renderer.render_retained(host).unwrap();
        renderer.poll().unwrap();
        renderer.collect_retained(host).unwrap();
        if let Some(RetainedEvent::Capture {
            id: actual,
            outcome: RetainedCaptureOutcome::Completed { image, .. },
            ..
        }) = host.captures.take_event()
        {
            assert_eq!(actual, id);
            return image;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[test]
fn native_rcas_recipe_is_applied_and_ordinary_frames_restore_the_profile() {
    let mut host = edge_host();
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    let ordinary = completed(&mut host, &mut renderer, recipe(1, 1, CaptureJitter::None));
    assert_edge(&ordinary);
    let settings = crate::NativeRcasSettings {
        stops: 0.75,
        domain: crate::RcasDomain::EncodedOutput,
    };
    let id = host
        .request_capture_recipe(
            0,
            recipe(1, 1, CaptureJitter::None)
                .with_native_rcas(settings)
                .unwrap(),
        )
        .unwrap();
    renderer.render_retained(&mut host).unwrap();
    assert_rcas(&renderer, Some(settings));
    let sharpened = await_capture(&mut host, &mut renderer, id);
    assert_sharpened_edge(&ordinary, &sharpened);
    let restored = ordinary_capture(&mut host, &mut renderer);
    assert_rcas(&renderer, None);
    assert_eq!(ordinary.srgb8.unwrap(), restored);
}

#[test]
fn native_rcas_profile_is_available_to_interactive_presentation() {
    let mut host = edge_host();
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    let ordinary = completed(&mut host, &mut renderer, recipe(1, 1, CaptureJitter::None));
    assert_edge(&ordinary);
    let rcas = crate::NativeRcasSettings {
        stops: 0.75,
        domain: crate::RcasDomain::DisplayLinear,
    };
    host.options.frame_profile = crate::FrameProfile::HdrNativeRcas {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
        rcas,
    };
    let mut renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    assert_rcas(&renderer, Some(rcas));
    let first = completed(&mut host, &mut renderer, recipe(1, 1, CaptureJitter::None));
    assert_sharpened_edge(&ordinary, &first);
    let override_rcas = crate::NativeRcasSettings {
        domain: crate::RcasDomain::EncodedOutput,
        ..rcas
    };
    let overridden = completed(
        &mut host,
        &mut renderer,
        recipe(1, 1, CaptureJitter::None)
            .with_native_rcas(override_rcas)
            .unwrap(),
    );
    assert_sharpened_edge(&ordinary, &overridden);
    assert_ne!(first.srgb8, overridden.srgb8);
    let restored = ordinary_capture(&mut host, &mut renderer);
    assert_rcas(&renderer, Some(rcas));
    assert_eq!(first.srgb8.unwrap(), restored);
}

fn edge_host() -> RetainedHost {
    RetainedHost::new(
        edge_scene(),
        crate::ViewerOptions {
            size: [64, 64],
            pose: crate::CameraPose {
                eye_millimeters: [0, -2000, 0],
                target_millimeters: [0; 3],
            },
            frame_profile: crate::FrameProfile::Hdr {
                samples: 1,
                tone_map: crate::ToneMap::Reinhard,
            },
            render_clock: RenderClock::Fixed(Duration::ZERO),
            ..crate::tests::options()
        },
    )
    .unwrap()
}

fn edge_scene() -> RenderScene {
    let base = crate::renderer::retained_tests::source();
    // A black background clamps RCAS's permitted negative lobe to zero at the edge.
    let textures = [32, 224].map(|value| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![value, value, value, 255].into(),
    });
    let materials = [0, 1].map(|index| {
        SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(index),
            normal: None,
            roughness_metallic: None,
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro: 1_000_000,
            metalness_micro: 0,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: true,
        })
        .unwrap()
    });
    let objects = [0, 1].map(|index| {
        SceneObject::new(
            *base.geometry().objects()[0].transform(),
            MeshHandle::from_raw(index),
            MaterialHandle::from_raw(index),
        )
    });
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures.to_vec(), materials.to_vec()).unwrap(),
        geometry: SceneGeometry::new(
            vec![edge_half(-4000, 0), edge_half(0, 4000)],
            objects.to_vec(),
        )
        .unwrap(),
        terrain: None,
        effects: base.effects().cloned(),
    })
    .unwrap()
}

fn edge_half(left: i64, right: i64) -> SceneMesh {
    let vertices = [
        [left, 0, -4000],
        [right, 0, -4000],
        [right, 0, 4000],
        [left, 0, 4000],
    ]
    .map(|position_millimeters| SceneVertex {
        position_millimeters,
        normal_micro: [0, -1_000_000, 0],
        tangent_micro: None,
        texcoord_micro: [0; 2],
    });
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices.to_vec(), vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap()
}

fn pixel(products: &RetainedCaptureProducts, x: usize, y: usize) -> &[u8] {
    let image = products.srgb8.as_ref().unwrap();
    assert_eq!((image.viewport.width, image.viewport.height), (64, 64));
    &image.pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4]
}

fn assert_edge(products: &RetainedCaptureProducts) {
    for y in 16..48 {
        let dark = pixel(products, 31, y);
        let light = pixel(products, 32, y);
        assert_eq!(dark, pixel(products, 30, y));
        assert_eq!(light, pixel(products, 33, y));
        for channel in 0..3 {
            assert!(dark[channel] > 20, "dark half must not be black: {dark:?}");
            assert!(
                light[channel] < 235,
                "light half must not be clipped: {light:?}"
            );
            assert!(i16::from(light[channel]) - i16::from(dark[channel]) > 40);
        }
        assert_eq!((dark[3], light[3]), (255, 255));
    }
}

fn assert_sharpened_edge(ordinary: &RetainedCaptureProducts, sharpened: &RetainedCaptureProducts) {
    for y in 16..48 {
        for channel in 0..3 {
            let dark = pixel(ordinary, 31, y)[channel];
            let light = pixel(ordinary, 32, y)[channel];
            let sharper_dark = pixel(sharpened, 31, y)[channel];
            let sharper_light = pixel(sharpened, 32, y)[channel];
            // Exceed transfer/attachment rounding as well as checking the predicted signs.
            assert!(
                i16::from(sharper_light) - i16::from(sharper_dark)
                    > i16::from(light) - i16::from(dark) + 4
            );
            assert!(
                sharper_dark < dark,
                "RCAS must darken edge ({y}, {channel}): {sharper_dark} >= {dark}"
            );
            assert!(
                sharper_light > light,
                "RCAS must brighten edge ({y}, {channel}): {sharper_light} <= {light}"
            );
        }
    }
}
