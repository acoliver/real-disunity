//! The material mip bias in the frame block moves the decal albedo to a finer mip level. The
//! renderer derives the bias from its configuration, so the biased frame comes from a renderer
//! that reconstructs 32x32 into 64x64 temporally (smaller axis ratio 0.5, bias exactly -2) and the
//! reference from a plain 32x32 renderer (bias +0.0), both drawing the same scene at the same
//! camera. The decal albedo has a constant colour per mip level and the 1 m decal on a vertical
//! card spans `64 / 2^j` of the 32 pixels, so the implicit LOD of the 64 texel texture is exactly
//! the integer `j`.
use super::decals::decal;
use super::mip_bias::{LEVELS, mipped_texture};
use super::*;
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{SceneDecalAxis, SceneDecalMapping};
use worldrunner_render_post::{
    AttachmentView, FrameConfig, HdrPostSettings, PostResolve, Readback, ResourceRef, ToneMap,
    UpscaleMode, UpscaleSettings,
};

const RENDER: u32 = 32;
const HDR: TextureFormat = TextureFormat::Rgba16Float;
/// Largest linear step between adjacent level colours is 190/255 = 0.745. The hardware resolves
/// the LOD to 1/256 (0.0029 of that step), and the half float scene target rounds a value below 1
/// by at most 2^-11 = 0.0005. The sum, 0.0034, is rounded up to 0.005.
const BOUND: f32 = 0.005;

/// Camera distance in millimeters at which a 1 m decal spans `64 / 2^level` of 32 pixels under
/// the 60 degree vertical field of view: `2^level / (4 tan 30 degrees)` meters, 1732.05 and
/// 3464.10, rounded to a millimeter. The LOD moves by under 5e-4.
fn distance_for(level: u32) -> i64 {
    match level {
        2 => 1732,
        3 => 3464,
        _ => unreachable!("only levels 2 and 3 have a fixture distance"),
    }
}

fn scene() -> RenderScene {
    let projection = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(1),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let mut projected = decal();
    projected.axis = SceneDecalAxis::Y;
    projected.dimensions_millimeters = [1000; 3];
    projected.transform.position_millimeters = [0, 0, 1000];
    projected.edge_fade_micro = 0;
    projected.angle_cutoff_micro = 0;
    projected.angle_fade_micro = 0;
    projected.mapping = SceneDecalMapping {
        double_sided: true,
        ..SceneDecalMapping::default()
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture(), mipped_texture()],
            vec![material(SceneAlphaMode::Opaque, false), projection],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![card(None)],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
    .with_decals(vec![projected])
    .unwrap()
}

fn frame_at(distance: i64) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -distance, 1000],
            target_millimeters: [0, 0, 1000],
        },
        camera(),
        Viewport {
            width: RENDER,
            height: RENDER,
        },
    )
    .unwrap();
    FrameUniform::unlit(&view)
}

fn plain(context: &GpuContext) -> ConfiguredFrameRenderer {
    let mut settings = RendererConfiguration::legacy([RENDER; 2], COLOR_FORMAT);
    settings.frame = FrameConfig::new([RENDER; 2], HDR, 1);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap()
}

fn temporal(context: &GpuContext) -> ConfiguredFrameRenderer {
    let output = [2 * RENDER; 2];
    let mut settings = RendererConfiguration::legacy(output, COLOR_FORMAT);
    settings.frame = FrameConfig::new(output, HDR, 1).with_render_extent([RENDER; 2]);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.post = HdrPostSettings {
        resolve: PostResolve::taa(0.9),
        upscale: Some(UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    };
    ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap()
}

/// Draws one frame through the public encoding boundary and reads the resolved render target.
fn center(context: &GpuContext, renderer: &mut ConfiguredFrameRenderer, distance: i64) -> [f32; 3] {
    let scene = scene();
    let pipelines = ScenePipelines::new(context.device(), HDR);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    // The resident scene has no post effects of its own; it adopts the renderer's requested ones.
    gpu.post = renderer.configuration().post;
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
            (&mut gpu, &pipelines),
            (&color, &depth),
            frame_at(distance),
        )
        .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
    let image = Readback::new(
        context.device(),
        FrameConfig::new([RENDER; 2], HDR, 1),
        1_000_000,
    )
    .unwrap()
    .read(
        context.device(),
        context.submission(),
        renderer.targets().texture(),
    )
    .unwrap();
    let at = usize::try_from((RENDER / 2 * RENDER + RENDER / 2) * 8).unwrap();
    let half = |channel: usize| {
        let bits = u16::from_le_bytes([image[at + 2 * channel], image[at + 2 * channel + 1]]);
        half::f16::from_bits(bits).to_f32()
    };
    [half(0), half(1), half(2)]
}

fn assert_level(actual: [f32; 3], level: usize, what: &str) {
    for (got, wanted) in actual.iter().zip(LEVELS[level]) {
        assert!(
            (got - f32::from(wanted) / 255.0).abs() <= BOUND,
            "{what}: {actual:?} is not level {level} {:?}",
            LEVELS[level]
        );
    }
}

#[test]
fn a_temporal_half_scale_renderer_selects_decal_mips_two_levels_finer() {
    let context = context();
    for level in [2_u32, 3] {
        let distance = distance_for(level);
        let reference = center(&context, &mut plain(&context), distance);
        assert_level(reference, level as usize, "bias 0");
        let mut configured = temporal(&context);
        let biased = center(&context, &mut configured, distance);
        assert_level(biased, level as usize - 2, "bias -2");
    }
}
