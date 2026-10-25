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
fn capture(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    size: [u32; 2],
    samples: u32,
    contribution: &WorldSceneContribution,
) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut renderer = FrameRenderer::with_output(
        context.device(),
        FrameConfig::new(size, format, samples),
        format,
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
    encoder.finish().submit().unwrap();
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
