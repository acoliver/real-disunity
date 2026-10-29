use super::*;
use crate::{
    CaptureJitter, CaptureOutput, RetainedCaptureOutcome, RetainedCaptureProducts,
    RetainedCaptureRecipe, RetainedEvent,
};
use worldrunner_render_plan::*;
use worldrunner_render_scene::offscreen::LinearRgbaImage;

#[test]
fn specular_linear_alpha_stores_authored_channel_before_lighting_remap() {
    for decal in [false, true] {
        let scene = source_with_roughness(0, decal, [250_000; 2]);
        let effects =
            scene
                .effects()
                .unwrap()
                .clone()
                .with_specular_convention(SceneSpecularConvention {
                    roughness: SceneRoughnessRemap::LinearAlpha,
                    ..Default::default()
                });
        let scene = scene
            .with_resources(
                scene.resources().clone(),
                scene.geometry().clone(),
                Some(effects),
            )
            .unwrap();
        assert_stored_roughness(scene, 0.25);
    }
}

#[test]
fn specular_zero_roughness_is_not_lighting_clamped_with_or_without_decal() {
    assert_stored_roughness(source_with_roughness(0, false, [0, 250_000]), 0.0);
    assert_stored_roughness(source_with_roughness(0, true, [500_000, 0]), 0.0);
}

fn assert_stored_roughness(scene: RenderScene, expected: f32) {
    let (mut host, mut renderer) = setup_scene(scene, 1);
    let products = capture(&mut host, &mut renderer, true);
    let plane = products.specular.as_ref().unwrap();
    let image = LinearRgbaImage::from_native(
        plane.viewport,
        wgpu::TextureFormat::Rgba16Float,
        &plane.bytes,
    )
    .unwrap();
    let normals = &products.normals.as_ref().unwrap().pixels;
    let mut covered = 0;
    for (pixel, normal) in image.pixels.iter().zip(normals) {
        if normal[3] != 0.0 {
            assert_eq!(pixel[3].to_bits(), expected.to_bits());
            covered += 1;
        }
    }
    assert!(covered > 20);
}

fn source(metalness: u32, decal: bool) -> RenderScene {
    source_with_roughness(metalness, decal, [500_000, 250_000])
}

fn source_with_roughness(metalness: u32, decal: bool, roughness: [u32; 2]) -> RenderScene {
    let base = crate::renderer::retained_tests::source();
    let textures = [[255; 4], [255, 0, 0, 255]].map(|rgba| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: rgba.to_vec().into(),
    });
    let materials = [
        (0, metalness, roughness[0]),
        (1, 1_000_000 - metalness, roughness[1]),
    ]
    .map(|(albedo, metalness_micro, roughness_micro)| {
        SceneMaterial::new(SceneMaterialParts {
            albedo: TextureHandle::from_raw(albedo),
            normal: None,
            roughness_metallic: None,
            ambient_occlusion: None,
            self_illumination: None,
            roughness_micro,
            metalness_micro,
            alpha_mode: SceneAlphaMode::Opaque,
            double_sided: false,
        })
        .unwrap()
    });
    let scene = base
        .with_resources(
            SceneResources::new(textures.to_vec(), materials.to_vec()).unwrap(),
            base.geometry().clone(),
            base.effects().cloned(),
        )
        .unwrap();
    scene
        .with_decals(if decal {
            vec![projection(&base)]
        } else {
            vec![]
        })
        .unwrap()
}

fn projection(base: &RenderScene) -> SceneDecal {
    SceneDecal {
        priority: 0,
        transform: *base.geometry().objects()[0].transform(),
        dimensions_millimeters: [4000; 3],
        material: MaterialHandle::from_raw(1),
        axis: SceneDecalAxis::Z,
        edge_fade_micro: 0,
        angle_cutoff_micro: 0,
        angle_fade_micro: 0,
        receivers: SceneDecalReceivers::ALL,
        mapping: SceneDecalMapping::default(),
        write_mask: worldrunner_render_plan::SceneDecalWriteMask::default(),
        alpha_test_cutoff_micro: None,
    }
}

fn setup(metalness: u32, decal: bool, samples: u32) -> (RetainedHost, Renderer) {
    setup_scene(source(metalness, decal), samples)
}

fn setup_scene(scene: RenderScene, samples: u32) -> (RetainedHost, Renderer) {
    let host = RetainedHost::new(
        scene,
        crate::ViewerOptions {
            size: [32, 32],
            frame_profile: crate::FrameProfile::Hdr {
                samples,
                tone_map: crate::ToneMap::Reinhard,
            },
            ssr: Some(SceneSsr {
                enabled: true,
                ..Default::default()
            }),
            ..crate::tests::options()
        },
    )
    .unwrap();
    let renderer = pollster::block_on(Renderer::windowless(&host)).unwrap();
    (host, renderer)
}

fn capture(
    host: &mut RetainedHost,
    renderer: &mut Renderer,
    specular: bool,
) -> RetainedCaptureProducts {
    let mut outputs = vec![
        CaptureOutput::Srgb8,
        CaptureOutput::LinearHdr,
        CaptureOutput::Normals,
        CaptureOutput::Depth,
        CaptureOutput::AmbientOcclusion,
        CaptureOutput::SsrConfidence,
    ];
    if specular {
        outputs.push(CaptureOutput::Specular);
    }
    let recipe =
        RetainedCaptureRecipe::new(1, 1, CaptureJitter::None, false, outputs, 1024 * 1024).unwrap();
    host.request_capture_recipe(0, recipe).unwrap();
    assert!(renderer.render_retained(host).unwrap());
    renderer.drain().unwrap();
    renderer.collect_retained(host).unwrap();
    let Some(RetainedEvent::Capture {
        outcome: RetainedCaptureOutcome::RecipeCompleted { products, .. },
        ..
    }) = host.captures.take_event()
    else {
        panic!("recipe result")
    };
    products
}

fn assert_oracle(products: &RetainedCaptureProducts, metalness: u32, decal: bool) {
    let plane = products.specular.as_ref().unwrap();
    assert_eq!([plane.viewport.width, plane.viewport.height], [32, 32]);
    assert_eq!(plane.bytes.len(), 32 * 32 * 8);
    let normals = &products.normals.as_ref().unwrap().pixels;
    let image = LinearRgbaImage::from_native(
        plane.viewport,
        wgpu::TextureFormat::Rgba16Float,
        &plane.bytes,
    )
    .unwrap();
    let metal = f64::from(if decal {
        1_000_000 - metalness
    } else {
        metalness
    }) / 1_000_000.0;
    let albedo = if decal { [1.0, 0.0, 0.0] } else { [1.0; 3] };
    let expected = albedo.map(|a| 0.04 * (1.0 - metal) + a * metal);
    let mut covered = 0;
    for (pixel, normal) in image.pixels.iter().zip(normals) {
        if normal[3] == 0.0 {
            assert_eq!(pixel.map(f32::to_bits), [0, 0, 0, 1.0_f32.to_bits()]);
            continue;
        }
        covered += 1;
        for channel in 0..3 {
            // One binary16 ULP covers the permitted GPU attachment rounding modes.
            let ulp = if expected[channel] > 0.0 {
                2.0_f64.powf(expected[channel].log2().floor() - 10.0)
            } else {
                0.0
            };
            assert!((f64::from(pixel[channel]) - expected[channel]).abs() <= ulp);
        }
        assert_eq!(
            pixel[3].to_bits(),
            (if decal { 0.25_f32 } else { 0.5_f32 }).to_bits()
        );
    }
    assert!(covered > 20);
}

fn raw_hdr(renderer: &Renderer) -> Vec<u8> {
    let texture = renderer.presentation.scheduler.targets().texture();
    let mut readback = worldrunner_render_post::Readback::new(
        renderer.context.device(),
        worldrunner_render_post::FrameConfig::new(
            [texture.width(), texture.height()],
            texture.format(),
            1,
        ),
        1024 * 1024,
    )
    .unwrap();
    readback
        .read(
            renderer.context.device(),
            renderer.context.submission(),
            texture,
        )
        .unwrap()
}

#[test]
fn resolved_specular_capture_matches_material_decal_oracles_and_repeats() {
    for (metalness, decal, samples) in [
        (0, false, 1),
        (1_000_000, false, 4),
        (500_000, false, 1),
        (0, true, 4),
        (1_000_000, true, 1),
    ] {
        let (mut host, mut renderer) = setup(metalness, decal, samples);
        let baseline = capture(&mut host, &mut renderer, false);
        assert!(baseline.specular.is_none());
        let mut previous = None;
        for _ in 0..5 {
            let mut products = capture(&mut host, &mut renderer, true);
            assert_oracle(&products, metalness, decal);
            let specular = products.specular.take().unwrap();
            assert_eq!(products, baseline);
            let native = (specular.bytes, raw_hdr(&renderer));
            if let Some(previous) = &previous {
                assert_eq!(&native, previous);
            }
            previous = Some(native);
        }
    }
}

#[test]
fn specular_admission_requires_metadata_and_rejects_accumulation() {
    let mut host = RetainedHost::new(
        crate::renderer::retained_tests::source(),
        crate::ViewerOptions {
            size: [32, 32],
            ..crate::tests::options()
        },
    )
    .unwrap();
    let recipe = RetainedCaptureRecipe::new(
        1,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Specular],
        1024 * 1024,
    )
    .unwrap();
    assert_eq!(
        host.request_capture_recipe(0, recipe),
        Err(crate::CaptureRequestError::FeatureUnavailable {
            output: CaptureOutput::Specular
        })
    );
    let (mut host, _renderer) = setup(0, false, 1);
    let recipe = RetainedCaptureRecipe::new(
        2,
        1,
        CaptureJitter::None,
        false,
        vec![CaptureOutput::Specular],
        1024 * 1024,
    )
    .unwrap();
    assert_eq!(
        host.request_capture_recipe(0, recipe),
        Err(crate::CaptureRequestError::UnsupportedRecipe)
    );
}
