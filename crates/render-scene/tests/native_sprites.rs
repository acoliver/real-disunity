use glam::Vec3;
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace,
    simulation::{SpriteTextures, extract_world},
};
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef};
use worldrunner_render_scene::{
    compositor::{FrameRenderer, encode_scene},
    device::GpuContext,
    draw::SceneGpu,
    frame::FrameUniform,
    renderer::ScenePipelines,
    simulation::WorldSceneContribution,
};
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
    presentation::{Camera, Sprite, SpriteRenderer},
};

fn contribution(size: [u32; 2]) -> WorldSceneContribution {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let camera = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(0., 0., -10.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    world
        .add_component(
            camera,
            ComponentDesc::Camera(Camera {
                half_height: 2.,
                clear: [0., 0., 0., 0.25],
                ..Default::default()
            }),
        )
        .unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::Sprite(SpriteRenderer {
                sprite: Some(Sprite {
                    identity: 0,
                    texture: 42,
                    rect: [0., 0., 1., 1.],
                    texture_size: [1, 1],
                    pivot: [0.5, 0.5],
                    pixels_per_unit: 0.5,
                }),
                color: [0.5, 1., 1., 0.5],
                ..Default::default()
            }),
        )
        .unwrap();
    let textures = SpriteTextures {
        textures: std::collections::BTreeMap::from([(
            42,
            SceneTexture::Texels {
                format: TexelFormat::Rgba8Unorm,
                color_space: TextureColorSpace::Srgb,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![128, 64, 32, 128].into_boxed_slice(),
            },
        )]),
    };
    WorldSceneContribution::new(extract_world(&world, &textures, size).unwrap()).unwrap()
}
struct Capture {
    image: Vec<u8>,
    passes: Vec<worldrunner_render_post::PassId>,
}
fn capture(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    size: [u32; 2],
    samples: u32,
    contribution: &WorldSceneContribution,
) -> Vec<u8> {
    capture_with_stages(context, gpu, pipelines, size, samples, contribution).image
}
fn capture_with_stages(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    size: [u32; 2],
    samples: u32,
    contribution: &WorldSceneContribution,
) -> Capture {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = FrameRenderer::with_output(
        context.device(),
        FrameConfig::new(size, format, samples),
        format,
    )
    .unwrap();
    renderer
        .configure_timing(
            context.device(),
            context.queue(),
            worldrunner_render_post::TimingMode::Optional,
        )
        .unwrap();
    let [r, g, b, a] = contribution.clear;
    renderer
        .set_clear_color(wgpu::Color { r, g, b, a })
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("sprite color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("sprite depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer.begin_timing(1).unwrap();
    encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (gpu, pipelines),
        (&color, &depth),
        FrameUniform::unlit(&contribution.view),
    )
    .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    renderer.finish_timing(&mut encoder).unwrap();
    encoder.finish().submit().unwrap();
    let image = read_image(context, &renderer, size);
    let passes = renderer
        .take_timings()
        .unwrap()
        .into_iter()
        .flat_map(|frame| frame.passes.into_iter().map(|pass| pass.pass_id))
        .collect();
    Capture { image, passes }
}
fn read_image(context: &GpuContext, renderer: &FrameRenderer, size: [u32; 2]) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    Readback::new(
        context.device(),
        FrameConfig::new(size, format, 1),
        1_000_000,
    )
    .unwrap()
    .read(
        context.device(),
        context.submission(),
        renderer.targets().texture(),
    )
    .unwrap()
}
#[test]
fn native_sprites_share_srgb_alpha_msaa_resize_and_unload() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
        )
        .unwrap();
        let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
        for size in [[32, 32], [48, 24]] {
            let frame = contribution(size);
            gpu.set_dynamic_contribution(
                context.device(),
                context.submission(),
                &pipelines,
                &frame.textures,
                &frame.quads,
            )
            .unwrap();
            let image = capture(&context, &mut gpu, &pipelines, size, samples, &frame);
            let center = usize::try_from((size[1] / 2 * size[0] + size[0] / 2) * 4).unwrap();
            for (&actual, expected) in image[center..center + 4].iter().zip([46u8, 30, 12, 112]) {
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "samples={samples} actual={actual} expected={expected}"
                );
            }
            assert_eq!(&image[..4], &[0, 0, 0, 64]);
            gpu.clear_dynamic();
            let image = capture(&context, &mut gpu, &pipelines, size, samples, &frame);
            assert_eq!(&image[center..center + 4], &[0, 0, 0, 64]);
        }
    }
}

fn glyph_frame(size: [u32; 2]) -> WorldSceneContribution {
    use worldrunner_render_plan::{
        simulation::{WorldRenderPlan, append_ui},
        ui::{UiGlyphQuad, UiRenderPlan, UiTextContribution},
    };
    use worldrunner_world_simulation::ui::UiLayout;
    let frame = contribution(size);
    let mut plan = WorldRenderPlan {
        camera_state: None,
        camera: Some(*frame.view.resolved_view()),
        clear: frame.clear,
        quads: vec![],
        textures: std::collections::BTreeMap::new(),
    };
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let pixels = glam::UVec2::from_array(size).as_vec2();
    let ui = UiRenderPlan {
        layout: UiLayout {
            viewport: size.into(),
            rects: vec![],
        },
        texts: vec![UiTextContribution {
            object,
            canvas: object,
            overlay: true,
            sorting_layer: 0,
            sorting_order: 0,
            color: [1., 1., 1., 0.5],
            texture: Some(SceneTexture::Texels {
                format: TexelFormat::R8Unorm,
                color_space: TextureColorSpace::Linear,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![128].into_boxed_slice(),
            }),
            quads: vec![UiGlyphQuad {
                corners: [
                    [0., 0., 0.],
                    [pixels.x, 0., 0.],
                    [pixels.x, pixels.y, 0.],
                    [0., pixels.y, 0.],
                ],
                uv: [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
            }],
        }],
    };
    append_ui(&mut plan, ui).unwrap();
    WorldSceneContribution::new(plan).unwrap()
}
#[test]
fn glyph_coverage_and_tint_alpha_reach_the_shared_srgb_target() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
        )
        .unwrap();
        let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
        let frame = glyph_frame([32, 32]);
        gpu.set_dynamic_contribution(
            context.device(),
            context.submission(),
            &pipelines,
            &frame.textures,
            &frame.quads,
        )
        .unwrap();
        let image = capture(&context, &mut gpu, &pipelines, [32, 32], samples, &frame);
        for (&actual, expected) in image[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4]
            .iter()
            .zip([137u8, 137, 137, 112])
        {
            assert!(
                actual.abs_diff(expected) <= 2,
                "samples={samples} actual={actual} expected={expected}"
            );
        }
    }
}

#[test]
fn native_global_lights_multiply_exact_gpu_pixels() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
        )
        .unwrap();
        let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
        for (lit, layer, active_light, expected) in [
            (true, 7, Some(true), [64, 128, 192, 255]),
            (true, 8, Some(true), [0, 0, 0, 255]),
            (false, 7, Some(true), [255, 255, 255, 255]),
            (true, 7, Some(false), [255, 255, 255, 255]),
            (true, 7, None, [255, 255, 255, 255]),
        ] {
            let frame = global_light_frame(lit, layer, active_light);
            gpu.set_dynamic_contribution(
                context.device(),
                context.submission(),
                &pipelines,
                &frame.textures,
                &frame.quads,
            )
            .unwrap();
            let image = capture(&context, &mut gpu, &pipelines, [16, 16], samples, &frame);
            let center = (8 * 16 + 8) * 4;
            assert_eq!(
                &image[center..center + 4],
                &expected,
                "samples={samples} lit={lit} layer={layer} enabled={active_light:?}"
            );
        }
    }
}

fn global_light_frame(lit: bool, layer: i32, active_light: Option<bool>) -> WorldSceneContribution {
    use worldrunner_world_simulation::presentation::GlobalLight2d;
    let mut world = global_light_world(lit, layer);
    for enabled in active_light.into_iter().flat_map(|enabled| [enabled; 2]) {
        let light = world.create_object(ObjectDesc::default()).unwrap();
        world
            .add_component(
                light,
                ComponentDesc::GlobalLight2d(GlobalLight2d {
                    enabled,
                    color: [0.012_817_365, 0.053_965_125, 0.131_778_78, 1.],
                    intensity: 2.,
                    sorting_layers: vec![7],
                    ..Default::default()
                }),
            )
            .unwrap();
    }
    let textures = SpriteTextures {
        textures: std::collections::BTreeMap::from([(
            1,
            SceneTexture::Texels {
                format: TexelFormat::Rgba8Unorm,
                color_space: TextureColorSpace::Linear,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![255; 4].into_boxed_slice(),
            },
        )]),
    };
    WorldSceneContribution::new(extract_world(&world, &textures, [16, 16]).unwrap()).unwrap()
}
fn global_light_world(lit: bool, layer: i32) -> World {
    use worldrunner_world_simulation::presentation::SpriteLighting;
    let mut world = World::new(WorldSettings::default()).unwrap();
    let camera = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(0., 0., -10.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    world
        .add_component(
            camera,
            ComponentDesc::Camera(Camera {
                half_height: 2.,
                ..Default::default()
            }),
        )
        .unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::Sprite(SpriteRenderer {
                lighting: if lit {
                    SpriteLighting::Global
                } else {
                    SpriteLighting::Unlit
                },
                sorting_layer: layer,
                sprite: Some(Sprite {
                    identity: 0,
                    texture: 1,
                    rect: [0., 0., 1., 1.],
                    pivot: [0.5; 2],
                    pixels_per_unit: 0.5,
                    texture_size: [1, 1],
                }),
                ..Default::default()
            }),
        )
        .unwrap();
    world
}

fn sampling_frame(
    sampler: Option<worldrunner_render_plan::TextureSampler>,
    u: f32,
) -> WorldSceneContribution {
    let world = global_light_world(false, 0);
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 1,
        mips: 1,
        bytes: vec![0, 0, 0, 255, 255, 255, 255, 255].into_boxed_slice(),
    };
    let texture = sampler.map_or_else(|| texture.clone(), |s| texture.clone().with_sampler(s));
    let textures = SpriteTextures {
        textures: std::collections::BTreeMap::from([(1, texture)]),
    };
    let mut frame =
        WorldSceneContribution::new(extract_world(&world, &textures, [16, 16]).unwrap()).unwrap();
    // Constant UVs and full pixel coverage make results independent of MSAA sample locations.
    frame.quads[0].uv_rect = [u, 0.5, u, 0.5];
    frame.quads[0].corners = Some([[-3., 0., -3.], [3., 0., -3.], [3., 0., 3.], [-3., 0., 3.]]);
    frame
}
fn sampling_cases() -> [(Option<worldrunner_render_plan::TextureSampler>, f32, u8); 7] {
    use worldrunner_render_plan::{TextureFilter as F, TextureSampler as S, TextureWrap as W};
    [
        (None, 0.375, 137),
        (Some(S::default()), 0.375, 137),
        (
            Some(S {
                filter: F::Point,
                ..Default::default()
            }),
            0.375,
            0,
        ),
        (
            Some(S {
                filter: F::Point,
                ..Default::default()
            }),
            1.25,
            0,
        ),
        (
            Some(S {
                filter: F::Point,
                wrap_u: W::Clamp,
                wrap_v: W::Clamp,
            }),
            1.25,
            255,
        ),
        (
            Some(S {
                filter: F::Point,
                wrap_u: W::Mirror,
                wrap_v: W::Mirror,
            }),
            1.25,
            255,
        ),
        (
            Some(S {
                filter: F::Point,
                wrap_u: W::Mirror,
                wrap_v: W::Mirror,
            }),
            1.75,
            0,
        ),
    ]
}
#[test]
fn per_texture_sampling_has_exact_pixels_and_default_output() {
    use worldrunner_render_plan::TextureSampler as S;
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
        )
        .unwrap();
        let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
        let mut default_image = None;
        for (sampler, u, expected) in sampling_cases() {
            let frame = sampling_frame(sampler, u);
            gpu.set_dynamic_contribution(
                context.device(),
                context.submission(),
                &pipelines,
                &frame.textures,
                &frame.quads,
            )
            .unwrap();
            let image = capture(&context, &mut gpu, &pipelines, [16, 16], samples, &frame);
            assert_eq!(
                &image[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
                &[expected, expected, expected, 255],
                "samples={samples} sampler={sampler:?} u={u}"
            );
            if sampler.is_none() {
                default_image = Some(image.clone());
            }
            if sampler == Some(S::default()) {
                assert_eq!(Some(&image), default_image.as_ref());
            }
        }
    }
}

#[test]
fn texture_samplers_are_independent_within_one_frame() {
    use worldrunner_render_plan::{TextureFilter, TextureSampler};
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
        )
        .unwrap();
        let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
        let mut frame = sampling_frame(None, 0.375);
        frame
            .textures
            .push(frame.textures[0].clone().with_sampler(TextureSampler {
                filter: TextureFilter::Point,
                ..Default::default()
            }));
        frame.quads[0].corners = Some([[-3., 0., -3.], [0., 0., -3.], [0., 0., 3.], [-3., 0., 3.]]);
        let mut point = frame.quads[0];
        point.corners = Some([[0., 0., -3.], [3., 0., -3.], [3., 0., 3.], [0., 0., 3.]]);
        point.texture = Some(1);
        frame.quads.push(point);
        gpu.set_dynamic_contribution(
            context.device(),
            context.submission(),
            &pipelines,
            &frame.textures,
            &frame.quads,
        )
        .unwrap();
        let image = capture(&context, &mut gpu, &pipelines, [16, 16], samples, &frame);
        assert_eq!(
            &image[(8 * 16 + 4) * 4..(8 * 16 + 4) * 4 + 4],
            &[137, 137, 137, 255]
        );
        assert_eq!(
            &image[(8 * 16 + 12) * 4..(8 * 16 + 12) * 4 + 4],
            &[0, 0, 0, 255]
        );
    }
}

#[test]
fn feature_off_pixels_and_stage_ids_remain_identical() {
    let context = pollster::block_on(GpuContext::request_with_features(
        wgpu::Features::TIMESTAMP_QUERY,
    ))
    .unwrap();
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
        )
        .unwrap();
        let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
        let mut baseline = None;
        for (lit, active_light) in [(false, None), (true, None), (true, Some(false))] {
            let frame = global_light_frame(lit, 7, active_light);
            gpu.set_dynamic_contribution(
                context.device(),
                context.submission(),
                &pipelines,
                &frame.textures,
                &frame.quads,
            )
            .unwrap();
            let capture =
                capture_with_stages(&context, &mut gpu, &pipelines, [16, 16], samples, &frame);
            assert!(!capture.passes.is_empty());
            if let Some((image, passes)) = &baseline {
                assert_eq!(&capture.image, image);
                assert_eq!(&capture.passes, passes);
            } else {
                baseline = Some((capture.image, capture.passes));
            }
        }
    }
}

fn mip_frame(sampler: Option<worldrunner_render_plan::TextureSampler>) -> WorldSceneContribution {
    let mut frame = sampling_frame(None, 0.5);
    let mut bytes = [0, 0, 0, 255].repeat(16 * 16);
    bytes.extend([255, 255, 255, 255].repeat(8 * 8));
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 16,
        height: 16,
        mips: 2,
        bytes: bytes.into_boxed_slice(),
    };
    frame.textures[0] =
        sampler.map_or_else(|| texture.clone(), |s| texture.clone().with_sampler(s));
    frame.quads[0].corners = Some([[-2., 0., -2.], [2., 0., -2.], [2., 0., 2.], [-2., 0., 2.]]);
    // LOD log2(1.25) ~= 0.322 blends mips, below the nearest-mip boundary at 0.5.
    frame.quads[0].uv_rect = [0., 0., 1.25, 1.25];
    frame
}
#[test]
fn authored_bilinear_selects_one_mip_and_unspecified_preserves_linear_mips() {
    use worldrunner_render_plan::TextureFilter;
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_samples(
            context.device(),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
        )
        .unwrap();
        let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
        let mut baseline = None;
        for sampler in mip_cases() {
            let frame = mip_frame(sampler);
            gpu.set_dynamic_contribution(
                context.device(),
                context.submission(),
                &pipelines,
                &frame.textures,
                &frame.quads,
            )
            .unwrap();
            let image = capture(&context, &mut gpu, &pipelines, [16, 16], samples, &frame);
            let pixel = &image[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4];
            if sampler.is_some_and(|s| s.filter == TextureFilter::Bilinear) {
                assert_eq!(
                    pixel,
                    &[0, 0, 0, 255],
                    "samples={samples} sampler={sampler:?}"
                );
            } else {
                // Fractional LOD precision varies by GPU; blending must stay between the mips.
                assert!(
                    pixel[..3]
                        .iter()
                        .all(|&channel| channel > 0 && channel < 255),
                    "samples={samples} sampler={sampler:?} pixel={pixel:?}"
                );
                assert_eq!(pixel, &[pixel[0], pixel[0], pixel[0], 255]);
            }
            if sampler.is_none() {
                baseline = Some(image);
            } else if sampler.is_some_and(|s| s.filter == TextureFilter::Trilinear) {
                assert_eq!(Some(&image), baseline.as_ref());
            }
        }
        assert!(baseline.is_some());
    }
}

fn mip_cases() -> [Option<worldrunner_render_plan::TextureSampler>; 3] {
    use worldrunner_render_plan::{TextureFilter as F, TextureSampler as S};
    [
        None,
        Some(S {
            filter: F::Trilinear,
            ..Default::default()
        }),
        Some(S {
            filter: F::Bilinear,
            ..Default::default()
        }),
    ]
}
