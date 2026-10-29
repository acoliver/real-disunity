use super::scene_tests;
use crate::{
    device::GpuContext,
    frame::FrameUniform,
    view::{CameraPose, SceneView, Viewport},
};
use worldrunner_render_plan::*;

fn source(alpha: u8, masked: bool, background: u32, meshes: bool) -> RenderScene {
    let textures = textures(alpha, background);
    let mut sky = scene_tests::atmosphere(0);
    sky.fog = None;
    sky.celestial_composition = SceneCelestialComposition::SeparateSurface;
    sky.sun_angular_radius_millidegrees = if background == 1 { 20_000 } else { 0 };
    if masked {
        let mut far = sky.meshes[0];
        far.distance_millimeters = 20_000;
        far.material = MaterialHandle::from_raw(0);
        sky.meshes.push(far);
    }
    if !meshes {
        sky.meshes.clear();
    }
    let near = masked_material(masked);
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![scene_tests::material(1), near]).unwrap(),
        geometry: SceneGeometry::new(vec![scene_tests::quad()], vec![]).unwrap(),
        terrain: None,
        effects: Some(
            scene_tests::base_effects(background == 2)
                .with_atmosphere(sky)
                .unwrap(),
        ),
    })
    .unwrap()
}

fn render(context: &GpuContext, scene: &RenderScene, samples: u32) -> Vec<u8> {
    use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef, ToneMap};
    let device = context.device();
    let format = wgpu::TextureFormat::Rgba16Float;
    let pipelines = crate::renderer::ScenePipelines::with_samples(device, format, samples).unwrap();
    let mut gpu =
        crate::draw::SceneGpu::upload(device, context.submission(), scene, &pipelines).unwrap();
    let mut renderer = crate::compositor::FrameRenderer::hdr(
        context,
        [32, 32],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        samples,
        ToneMap::Reinhard,
        scene.effects(),
    )
    .unwrap();
    let frame = frame(scene);
    let color = AttachmentView::new(
        ResourceRef::new("test color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("test depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(device).unwrap();
    crate::compositor::encode_scene(
        &mut renderer,
        device,
        &mut encoder,
        (&mut gpu, &pipelines),
        (&color, &depth),
        frame,
    )
    .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
    Readback::new(device, FrameConfig::new([32, 32], format, 1), 100_000)
        .unwrap()
        .read(
            device,
            context.submission(),
            renderer.targets().resolved().texture(),
        )
        .unwrap()
}

fn rgb(bytes: &[u8], x: usize, y: usize) -> [f32; 3] {
    std::array::from_fn(|c| {
        let i = y * 256 + x * 8 + c * 2;
        half::f16::from_le_bytes([bytes[i], bytes[i + 1]]).to_f32()
    })
}

#[test]
fn separate_surface_mask_preserves_far_mesh_below_cutoff() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let below = render(&context, &source(64, true, 0, true), samples);
        let above = render(&context, &source(192, true, 0, true), samples);
        let low = rgb(&below, 16, 16);
        let high = rgb(&above, 16, 16);
        assert!(low[0] > 0.99 && low[1] == 0.0, "{samples}x below: {low:?}");
        assert!(
            high[0] == 0.0 && high[1] > 0.7,
            "{samples}x above: {high:?}"
        );
    }
}

#[test]
fn zero_surface_preserves_solar_and_minified_dome_background() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for background in [1, 2] {
        for samples in crate::msaa_tests::supported_samples(&context) {
            let expected = render(&context, &source(255, false, background, false), samples);
            let actual = render(&context, &source(255, false, background, true), samples);
            for (x, y) in [(16, 16), (12, 12), (20, 20)] {
                let a = rgb(&actual, x, y);
                let b = rgb(&expected, x, y);
                for c in 0..3 {
                    assert!(
                        (a[c] - b[c]).abs() < 0.004,
                        "background {background} {samples}x ({x},{y}): {a:?} != {b:?}"
                    );
                }
            }
            if background == 1 {
                assert!(rgb(&expected, 16, 16)[0] > 2.9);
            } else {
                assert!(
                    rgb(&expected, 16, 16)[1] > 0.1,
                    "dome must select nonzero mip"
                );
            }
        }
    }
}

fn textures(alpha: u8, background: u32) -> Vec<SceneTexture> {
    let mut textures = scene_tests::textures().to_vec();
    textures[2] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0, if background == 0 { 255 } else { 0 }, 0, alpha].into(),
    };
    if background == 2 {
        let bytes = (0..9)
            .flat_map(|mip| {
                let color = if mip == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 255, 255]
                };
                color.repeat((256_usize >> mip).pow(2))
            })
            .collect::<Vec<_>>();
        textures[3] = SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 256,
            height: 256,
            mips: 9,
            bytes: bytes.into(),
        };
    }
    textures
}

fn masked_material(masked: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(2),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: Some(TextureHandle::from_raw(0)),
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: if masked {
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            }
        } else {
            SceneAlphaMode::Opaque
        },
        double_sided: true,
    })
    .unwrap()
}

fn frame(scene: &RenderScene) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0; 3],
            target_millimeters: [0, 1000, 0],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: 32,
            height: 32,
        },
    )
    .unwrap();
    let mut frame = FrameUniform::new(&view, scene.effects()).unwrap();
    frame.sun_direction = glam::Vec3::Y;
    frame.sun_irradiance = glam::Vec3::new(3.0, 2.0, 1.0);
    frame
}
