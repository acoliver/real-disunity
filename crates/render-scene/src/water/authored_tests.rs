#[test]
fn authored_map_representations_and_tiles_fail_before_allocation() {
    let mut body = settings();
    body.material.tile_millimeters = [0, 1000];
    assert!(body.validate().is_err());
    body.material.tile_millimeters = [1000; 2];
    body.material.normal = Some(TextureHandle::from_raw(0));
    for color_space in [TextureColorSpace::Linear, TextureColorSpace::Srgb] {
        let source = scene(
            vec![body.clone()],
            vec![SceneTexture::Texels {
                format: TexelFormat::Rgba8Unorm,
                color_space,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![128, 128, 255, 255].into_boxed_slice(),
            }],
        );
        assert_eq!(
            surface::validate(&source).is_ok(),
            color_space == TextureColorSpace::Linear
        );
    }
    body.material.refraction_millimeters = Some(0);
    assert!(validate_settings(&body).is_err());
}
fn filtered(u: f32, values: [u8; 4]) -> f32 {
    let x = u.rem_euclid(1.0) * 4.0;
    values
        .into_iter()
        .zip([0.5, 1.5, 2.5, 3.5])
        .map(|(v, center)| {
            let delta = (x - center).abs();
            f32::from(v) / 255.0 * (1.0 - delta.min(4.0 - delta)).max(0.0)
        })
        .sum()
}
fn check_normal(bytes: &[u8], samples: u32, sample: u32, time: f32) {
    let offset = if samples == 1 {
        [0.5, 0.5]
    } else {
        [
            [0.375, 0.125],
            [0.875, 0.375],
            [0.125, 0.625],
            [0.625, 0.875],
        ][usize::try_from(sample).unwrap()]
    };
    let frame = FrameUniform::new(&fixture_view(), None).unwrap();
    let inverse = frame.view_projection.inverse();
    for (x, y, right) in [(12u16, 16u16, false), (20, 16, true)] {
        let unproject = |z| {
            let p = inverse
                * glam::Vec4::new(
                    (f32::from(x) + offset[0]) / 16.0 - 1.0,
                    1.0 - (f32::from(y) + offset[1]) / 16.0,
                    z,
                    1.0,
                );
            p.truncate() / p.w
        };
        let a = unproject(0.0);
        let b = unproject(1.0);
        let p = a + (b - a) * (-a.z / (b.z - a.z));
        let velocity = filtered(p.x * 0.5, [0, 255, 0, 255]) * 2.0 - 1.0;
        let uv = (p.x - velocity * time) * 0.5;
        let q = filtered(uv, [64, 192, 64, 192]) * 2.0 - 1.0;
        let fixed = 128.0 / 255.0 * 2.0 - 1.0;
        let xy = if right {
            glam::Vec2::new(fixed, -q + 2.0 / 255.0)
        } else {
            glam::Vec2::new(q, fixed)
        };
        let world = xy
            .extend((1.0 - xy.length_squared()).max(0.0001).sqrt())
            .normalize();
        let expected = frame.view.transform_vector3(world).normalize();
        let base = (usize::from(y) * 32 + usize::from(x)) * 8;
        for (channel, target) in expected.to_array().into_iter().enumerate() {
            let index = base + channel * 2;
            let bits = u16::from_le_bytes(bytes[index..index + 2].try_into().unwrap());
            let value = numerical::positive_half(&(bits & 0x7fff).to_le_bytes())
                * if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
            assert!(
                (value - target).abs() < 0.006,
                "authored normal {samples}x sample {sample} time {time}: {value} versus {target}"
            );
        }
    }
}
use super::*;
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::*;

fn scene(bodies: Vec<SceneWaterBody>, textures: Vec<SceneTexture>) -> RenderScene {
    let white = SceneColor {
        red: 255,
        green: 255,
        blue: 255,
    };
    let effects = SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: SceneSky::ProceduralHemisphere {
            zenith: white,
            horizon: white,
        },
        sun: SceneSun {
            direction: SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            irradiance_micro: 0,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 0,
            ground_irradiance_micro: 0,
        },
        shadows: SceneShadowCascades::new(vec![10_000, 40_000], 50_000).unwrap(),
        ssao: None,
        camera: SceneCamera {
            vertical_fov_millidegrees: 60000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![],
        anti_alias: SceneAntiAlias::Disabled,
    })
    .unwrap()
    .with_water_bodies(bodies)
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}
fn configuration(samples: u32, bodies: Vec<SceneWaterBody>) -> RendererConfiguration {
    let mut config = RendererConfiguration::legacy([32, 32], wgpu::TextureFormat::Rgba8UnormSrgb);
    config.hdr = Some((1.0, ToneMap::Reinhard));
    config.frame = FrameConfig::new([32, 32], wgpu::TextureFormat::Rgba16Float, samples);
    config.water = bodies;
    config
}
fn encode(
    context: &GpuContext,
    renderer: &mut ConfiguredFrameRenderer,
    gpu: &mut crate::draw::SceneGpu,
    pipelines: &crate::renderer::ScenePipelines,
) {
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer
        .encoding()
        .encode_scene(
            context.device(),
            &mut encoder,
            (gpu, pipelines),
            (&color, &depth),
            FrameUniform::new(&fixture_view(), None).unwrap(),
        )
        .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
}
fn map(bytes: Vec<u8>) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rg8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 1,
        mips: 1,
        bytes: bytes.into_boxed_slice(),
    }
}
#[test]
fn configured_authored_maps_tile_scroll_and_differ_between_bodies() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
        let (bodies, source) = authored_scene();
        let pipelines = crate::renderer::ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba16Float,
            samples,
        )
        .unwrap();
        let mut gpu = crate::draw::SceneGpu::upload(
            context.device(),
            context.submission(),
            &source,
            &pipelines,
        )
        .unwrap();
        let mut renderer = ConfiguredFrameRenderer::new(
            &context,
            configuration(samples, bodies),
            AllocationBudget(10_000_000),
        )
        .unwrap();
        let mut outputs = Vec::new();
        for time in [0.0, 0.375] {
            renderer.set_frame_time(time).unwrap();
            encode(&context, &mut renderer, &mut gpu, &pipelines);
            let (normal, _, _) = renderer.water_receivers().unwrap();
            let bytes =
                lighting_tests::environment_tests::sample_read(&context, normal.texture(), 0);
            assert_ne!(
                &bytes[(16 * 32 + 12) * 8..(16 * 32 + 12) * 8 + 6],
                &bytes[(16 * 32 + 20) * 8..(16 * 32 + 20) * 8 + 6],
                "body maps remain independent"
            );
            for sample in 0..samples {
                let data = lighting_tests::environment_tests::sample_read(
                    &context,
                    normal.texture(),
                    sample,
                );
                check_normal(&data, samples, sample, time);
            }
            outputs.push(bytes);
        }
        assert_ne!(
            outputs[0], outputs[1],
            "authored flow scrolls the sampled normal field"
        );
    }
}
#[test]
fn configured_environment_survives_extent_replacement() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let maps = lighting_tests::environment_tests::prepare_environment(&context);
    for samples in [1, 4] {
        let mut body = settings();
        body.color_micro = [0; 3];
        body.absorption_micro = [10_000_000; 3];
        let source = scene(vec![body.clone()], vec![]);
        let pipelines = crate::renderer::ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba16Float,
            samples,
        )
        .unwrap();
        let mut gpu = crate::draw::SceneGpu::upload(
            context.device(),
            context.submission(),
            &source,
            &pipelines,
        )
        .unwrap();
        let mut config = configuration(samples, vec![body]);
        let mut renderer =
            ConfiguredFrameRenderer::new(&context, config.clone(), AllocationBudget(10_000_000))
                .unwrap();
        renderer.set_water_environment(&maps).unwrap();
        encode(&context, &mut renderer, &mut gpu, &pipelines);
        let before = read(&context, renderer.targets().texture());
        assert!(before.chunks_exact(8).any(|p| p[0] != 0 || p[1] != 0));
        config.frame.size = [40, 32];
        renderer.reconfigure(&context, config.clone()).unwrap();
        assert!(renderer.water_receivers().is_none());
        encode(&context, &mut renderer, &mut gpu, &pipelines);
        config.frame.size = [32, 32];
        renderer.reconfigure(&context, config).unwrap();
        encode(&context, &mut renderer, &mut gpu, &pipelines);
        assert_eq!(before, read(&context, renderer.targets().texture()));
        assert!(renderer.water_receivers().is_some());
    }
}

fn authored_scene() -> (Vec<SceneWaterBody>, RenderScene) {
    let mut left = settings();
    left.east_min_millimeters = -10000;
    left.east_extent_millimeters = 10000;
    left.material.normal = Some(TextureHandle::from_raw(0));
    left.material.flow = Some(TextureHandle::from_raw(2));
    left.material.tile_millimeters = [2000; 2];
    let mut right = left.clone();
    right.east_min_millimeters = 0;
    right.material.normal = Some(TextureHandle::from_raw(1));
    let bodies = vec![left, right];
    let source = scene(
        bodies.clone(),
        vec![
            map(vec![64, 128, 192, 128, 64, 128, 192, 128]),
            map(vec![128, 192, 128, 64, 128, 192, 128, 64]),
            map(vec![0, 128, 255, 128, 0, 128, 255, 128]),
        ],
    );
    (bodies, source)
}
