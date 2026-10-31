use super::*;

mod baseline;

#[test]
fn gpu_water_absent_response_preserves_base_colour_bytes() {
    let context = context();
    let original = baseline::source();
    for scene_path in [false, true] {
        let expected = capture_source(&context, None, scene_path, false, Some(&original));
        let actual = capture(&context, None, scene_path, false);
        assert_eq!(
            baseline::hash(&actual),
            baseline::hash(&expected),
            "scene={scene_path}"
        );
        assert_eq!(actual, expected, "scene={scene_path}");
    }
}

#[test]
fn gpu_water_specialized_neutral_is_identical_with_msaa_and_rippling_normals() {
    let context = context();
    for scene_path in [false, true] {
        for raw in [true, false] {
            let baseline = capture(&context, None, scene_path, raw);
            assert!(
                baseline
                    .chunks_exact(8)
                    .any(|pixel| pixel != &baseline[..8])
            );
            for offset in [
                SceneWaterShadowOffset::ShadingNormal,
                SceneWaterShadowOffset::GeometricNormal,
                SceneWaterShadowOffset::None,
            ] {
                let response = SceneWaterShadowResponse {
                    offset,
                    ..Default::default()
                };
                let actual = capture(&context, Some(response), scene_path, raw);
                let differences = baseline.iter().zip(&actual).filter(|(a, b)| a != b).count();
                assert_eq!(
                    differences, 0,
                    "scene={scene_path}, raw={raw}, offset={offset:?}"
                );
            }
        }
    }
}

fn source(response: Option<SceneWaterShadowResponse>, scene_path: bool) -> RenderScene {
    let base = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut water = unsupported_water_body();
    water.height_millimeters = 1;
    water.east_min_millimeters = -3000;
    water.north_min_millimeters = -3000;
    water.east_extent_millimeters = 6000;
    water.north_extent_millimeters = 6000;
    water.material.direct_surface = true;
    water.material.normal = Some(TextureHandle::from_raw(
        u32::try_from(base.resources().textures().len()).unwrap(),
    ));
    water.material.shadow_response = response;
    let mut textures = base.resources().textures().to_vec();
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rg8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 1,
        mips: 1,
        bytes: vec![32, 128, 64, 128, 192, 128, 224, 128].into_boxed_slice(),
    });
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, base.resources().materials().to_vec()).unwrap(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(effects(water, scene_path)),
    })
    .unwrap()
}

fn effects(water: SceneWaterBody, scene_path: bool) -> SceneEffects {
    let mut effects = fixture::effects_parts(600_000);
    effects.sun.direction = SceneDirection::new([360_000, 480_000, 800_000]).unwrap();
    effects.shadows = SceneShadowCascades::new(vec![2_000, 4_000, 8_000, 16_000], 100_000)
        .unwrap()
        .with_sampling(SceneShadowSampling {
            caster_slope_bias_micro: 3_000,
            filter: SceneShadowFilter::Bilinear2,
            strength_micro: 800_000,
        })
        .unwrap();
    effects.water = Some(water);
    let convention = if scene_path {
        SceneSpecularConvention {
            roughness: SceneRoughnessRemap::LinearAlpha,
            visibility: SceneSpecularVisibility::SchlickSmith,
            occlusion: SceneSpecularOcclusion::RoughnessAware,
            direct_diffuse: SceneDirectDiffuse::LambertianPrepared,
            ..Default::default()
        }
    } else {
        SceneSpecularConvention::default()
    };
    SceneEffects::new(effects)
        .unwrap()
        .with_specular_convention(convention)
}

fn capture(
    context: &GpuContext,
    response: Option<SceneWaterShadowResponse>,
    scene_path: bool,
    raw: bool,
) -> Vec<u8> {
    capture_source(context, response, scene_path, raw, None)
}

fn capture_source(
    context: &GpuContext,
    response: Option<SceneWaterShadowResponse>,
    scene_path: bool,
    raw: bool,
    shader: Option<&str>,
) -> Vec<u8> {
    use worldrunner_render_post::{FrameConfig, Readback, ToneMap};
    let source = source(response, scene_path);
    let view = view(&source);
    let mut frame = crate::compositor::FrameRenderer::hdr(
        context,
        [256, 256],
        OffscreenTarget::COLOR_FORMAT,
        4,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    crate::water::shadow_probe::install_source(
        context.device(),
        &mut frame,
        source.effects().unwrap().specular_convention(),
        raw,
        shader.map_or_else(|| crate::water::water_source(true), str::to_owned),
    );
    submit(context, &source, &view, &mut frame);
    Readback::new(
        context.device(),
        FrameConfig::new([256, 256], wgpu::TextureFormat::Rgba16Float, 1),
        1_000_000,
    )
    .unwrap()
    .read(
        context.device(),
        context.submission(),
        frame.targets().texture(),
    )
    .unwrap()
}

fn view(source: &RenderScene) -> SceneView {
    SceneView::new(
        crate::CameraPose {
            eye_millimeters: [2333, -4217, 6751],
            target_millimeters: [137, -259, 0],
        },
        *source.effects().unwrap().camera(),
        crate::Viewport {
            width: 256,
            height: 256,
        },
    )
    .unwrap()
}

fn submit(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    frame: &mut crate::compositor::FrameRenderer,
) {
    let mut renderer = ShadowedScene::with_target(
        context,
        source,
        DirectionalShadowSettings {
            depth_bias: 0.0,
            normal_bias_meters: 0.0,
            ..settings()
        },
        wgpu::TextureFormat::Rgba16Float,
        4,
    )
    .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer
        .encode_with(context, view, (&color, &depth), &mut encoder, Some(frame))
        .unwrap();
    frame.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
}
