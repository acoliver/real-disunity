//! Native world contributions, which reach the dynamic path as sprites and as UI glyphs, drawn
//! through the frame renderer's scene entry point.

use glam::Vec3;
use worldrunner_render_plan::simulation::{
    SpriteTextures, WorldRenderPlan, append_ui, extract_world,
};
use worldrunner_render_plan::ui::{UiGlyphQuad, UiRenderPlan, UiTextContribution};
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef};
use worldrunner_world_simulation::presentation::{Camera, CoverageTexture, Sprite, SpriteRenderer};
use worldrunner_world_simulation::ui::UiLayout;
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
};

use super::*;
use crate::compositor::{FrameRenderer, encode_scene};
use crate::simulation::WorldSceneContribution;

const SPRITE: TextureFormat = SRGB;

pub(super) fn sprite_frame() -> WorldSceneContribution {
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
                background: [0., 0., 0., 0.25],
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
    WorldSceneContribution::new(extract_world(&world, &textures, [SIZE, SIZE]).unwrap()).unwrap()
}

pub(super) fn glyph_frame() -> WorldSceneContribution {
    let frame = sprite_frame();
    let mut plan = WorldRenderPlan {
        camera_state: None,
        camera: Some(*frame.view.resolved_view()),
        clear: frame.clear,
        quads: vec![],
        textures: std::collections::BTreeMap::new(),
    };
    let mut world = World::new(WorldSettings::default()).unwrap();
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let pixels = f32::from(u16::try_from(SIZE).unwrap());
    let ui = UiRenderPlan {
        layout: UiLayout {
            viewport: [SIZE, SIZE].into(),
            rects: vec![],
        },
        graphics: vec![],
        texts: vec![UiTextContribution {
            object,
            canvas: object,
            hierarchy_order: 0,
            clip_rect: [0, 0, SIZE, SIZE],
            overlay: true,
            sorting_layer: 0,
            sorting_order: 0,
            color: [1., 1., 1., 0.5],
            texture: Some(CoverageTexture {
                width: 1,
                height: 1,
                bytes: vec![128].into(),
            }),
            quads: vec![UiGlyphQuad {
                link: None,
                color: [1.; 4],
                corners: [
                    [0., 0., 0.],
                    [pixels, 0., 0.],
                    [pixels, pixels, 0.],
                    [0., pixels, 0.],
                ],
                uv: [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
            }],
        }],
    };
    append_ui(&mut plan, ui).unwrap();
    WorldSceneContribution::new(plan).unwrap()
}

/// The frame renderer's output for `contribution`, as the native sprite tests capture it.
fn compositor(
    context: &GpuContext,
    gpu: &mut SceneGpu,
    pipelines: &ScenePipelines,
    samples: u32,
    contribution: &WorldSceneContribution,
) -> Vec<u8> {
    let size = [SIZE, SIZE];
    let mut renderer = FrameRenderer::with_output(
        context.device(),
        FrameConfig::new(size, SPRITE, samples),
        SPRITE,
    )
    .unwrap();
    let [r, g, b, a] = contribution.clear;
    renderer
        .set_clear_color(wgpu::Color { r, g, b, a })
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("capture color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("capture depth"),
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
        FrameConfig::new(size, SPRITE, 1),
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

pub(super) fn captures(context: &GpuContext, name: &str, frame: &WorldSceneContribution) {
    for samples in [1, 4] {
        let pipelines = ScenePipelines::with_samples(context.device(), SPRITE, samples).unwrap();
        let mut gpu = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
        gpu.set_dynamic_contribution(
            context.device(),
            context.submission(),
            &pipelines,
            &frame.textures,
            &frame.quads,
        )
        .unwrap();
        let scene = compositor(context, &mut gpu, &pipelines, samples, frame);
        let legacy = Legacy::new(
            context,
            &pipelines,
            &frame.textures,
            &frame.quads,
            &frame.view,
        );
        let [r, g, b, a] = frame.clear;
        let reference = render_cleared(
            context,
            (&[SPRITE], samples),
            Some(wgpu::Color { r, g, b, a }),
            |pass| legacy.draw(pass),
        );
        assert!(
            scene == reference[0],
            "{name} at {samples} samples differs from the fixed pipeline's output"
        );
        let clear_only = render_cleared(
            context,
            (&[SPRITE], samples),
            Some(wgpu::Color { r, g, b, a }),
            |_| {},
        );
        assert!(scene != clear_only[0], "{name} drew nothing");
        report(context, &format!("{name}-{samples}x"), &[scene]);
    }
}
