//! Stock scheduling, temporal identity and immutable products across water, resize and slots.
use super::*;
use worldrunner_render_post::{AttachmentView, ResourceRef, ToneMap};
#[path = "depth_reuse_transitions.rs"]
mod transitions;

fn source() -> RenderScene {
    let base = terrain_scene(1);
    let mut parts = effect_parts();
    parts.anti_alias = SceneAntiAlias::taa(900_000);
    parts.ssao = Some(crate::ssr::neutral_ao());
    let mut atmosphere = crate::sky::scene_tests::atmosphere(0);
    atmosphere.meshes[0].material = MaterialHandle::from_raw(0);
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_ssr(worldrunner_render_plan::SceneSsr {
            enabled: true,
            ..Default::default()
        })
        .unwrap()
        .with_atmosphere(atmosphere)
        .unwrap()
        .with_water_bodies(vec![worldrunner_render_plan::SceneWaterBody {
            material: worldrunner_render_plan::SceneWaterSettings::STOCK,
            surface: worldrunner_render_plan::SceneWaterSurface::Rectangle,
            flow_millimeters_per_second: [0; 2],
            height_millimeters: -100,
            east_min_millimeters: -4000,
            north_min_millimeters: -4000,
            east_extent_millimeters: 8000,
            north_extent_millimeters: 8000,
            color_micro: [50_000, 200_000, 250_000],
            absorption_micro: [300_000, 100_000, 50_000],
            roughness_micro: 200_000,
        }])
        .unwrap();
    base.with_objects_and_effects(base.geometry().objects().to_vec(), Some(effects))
        .unwrap()
}

fn sequence(context: &GpuContext, independent: bool, shadows: bool) -> Vec<Vec<Vec<u8>>> {
    let source = source();
    let device = context.device();
    let pipelines =
        ScenePipelines::with_ssao_taa_velocity(device, TextureFormat::Rgba16Float, 1).unwrap();
    let mut renderer = FrameRenderer::hdr(
        context,
        SIZE,
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    if independent {
        renderer.independent_depth_for_test(device);
    }
    let mut gpu = SceneGpu::upload(device, context.submission(), &source, &pipelines).unwrap();
    stage_quad(context, &mut gpu, &pipelines);
    let mut shadowed = shadows.then(|| {
        crate::shadows::ShadowedScene::with_taa_velocity(
            context,
            &source,
            shadow_settings(),
            TextureFormat::Rgba16Float,
            1,
        )
        .unwrap()
    });
    if let Some(shadowed) = &mut shadowed {
        let (gpu, pipelines) = shadowed.scene_and_pipelines_mut();
        stage_quad(context, gpu, pipelines);
    }
    [0.0, 0.125, 0.25, 0.375]
        .into_iter()
        .enumerate()
        .map(|(index, time)| {
            draw_frame(
                context,
                &mut renderer,
                (&mut gpu, &pipelines),
                shadowed.as_mut(),
                &source,
                index,
                time,
            )
        })
        .collect()
}

fn stage_quad(context: &GpuContext, gpu: &mut SceneGpu, pipelines: &ScenePipelines) {
    gpu.set_dynamic_contribution(
        context.device(),
        context.submission(),
        pipelines,
        &[],
        &[crate::dynamic::DynamicQuad {
            position: [0.0, -0.5, 0.7],
            tint: [0.3, 0.8, 0.2, 0.5],
            ..Default::default()
        }],
    )
    .unwrap();
}

fn draw_frame(
    context: &GpuContext,
    renderer: &mut FrameRenderer,
    gpu: (&mut SceneGpu, &ScenePipelines),
    mut shadowed: Option<&mut crate::shadows::ShadowedScene>,
    source: &RenderScene,
    index: usize,
    time: f32,
) -> Vec<Vec<u8>> {
    let device = context.device();
    let (gpu, pipelines) = gpu;
    if index == 2 {
        renderer.resize(device, [67, 51]).unwrap();
    }
    if index == 3 {
        renderer.resize(device, SIZE).unwrap();
    }
    renderer.set_frame_time(time).unwrap();
    renderer.select_frame_slot(index % 3);
    if let Some(shadowed) = &mut shadowed {
        shadowed.select_frame_slot(index % 3);
    }
    let size = renderer.targets().config().size;
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
    let output =
        worldrunner_render_post::color_texture(device, "temporal differential", size, COLOR_FORMAT);
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: size[0],
            height: size[1],
        },
    )
    .unwrap();
    let frame = FrameUniform::new(&view, source.effects()).unwrap();
    context
        .submission()
        .prepare(device, |encoder, _| {
            if let Some(shadowed) = &mut shadowed {
                shadowed
                    .encode_with(context, &view, (&color, &depth), encoder, Some(renderer))
                    .unwrap();
            } else {
                encode_scene(
                    renderer,
                    device,
                    encoder,
                    (gpu, pipelines),
                    (&color, &depth),
                    frame,
                )
                .unwrap();
            }
            renderer
                .encode_output(
                    encoder,
                    &output.create_view(&TextureViewDescriptor::default()),
                )
                .unwrap();
            Ok::<_, worldrunner_render_post::FrameError>(())
        })
        .unwrap()
        .submit()
        .unwrap();
    let mut products = frame_products(context, renderer, index % 3);
    products.push(capture::read(context, &output));
    products
}

fn frame_products(context: &GpuContext, renderer: &FrameRenderer, slot: usize) -> Vec<Vec<u8>> {
    let screen = renderer.ssao.as_ref().unwrap();
    let reflections = renderer.ssr.as_ref().unwrap();
    let snapshots = &renderer.snapshots.as_ref().unwrap()[slot];
    let mut textures = vec![
        renderer.targets().scene().texture(),
        renderer.targets().depth().texture(),
        &screen.normals,
        reflections.material_target(),
        screen.passes.depth_levels()[0],
        renderer.targets().velocity_texture().unwrap(),
    ];
    assert_ne!(
        capture::read(context, &snapshots.depth),
        capture::read(context, &snapshots.final_depth),
        "water fixture must write depth in front of opaque receivers"
    );
    textures.extend([&snapshots.color, &snapshots.depth, &snapshots.final_depth]);
    textures.extend(reflections.textures());
    textures.extend(renderer.water.as_ref().unwrap().receivers[slot].iter());
    textures
        .iter()
        .map(|texture| capture::read(context, texture))
        .collect()
}

#[test]
fn gpu_paired_depth_preserves_water_sky_temporal_slots_resize_and_shadows() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for shadows in [false, true] {
        assert_eq!(
            sequence(&context, true, shadows),
            sequence(&context, false, shadows),
            "shadows={shadows}"
        );
    }
}

fn shadow_settings() -> crate::shadows::DirectionalShadowSettings {
    crate::shadows::DirectionalShadowSettings {
        resolution: 32,
        focus_meters: glam::Vec3::ZERO,
        half_extent_meters: 10.0,
        depth_half_extent_meters: 10.0,
        depth_bias: 0.0,
        normal_bias_meters: 0.0,
        refresh_policy: crate::shadows::CascadeRefreshPolicy::default(),
    }
}

fn sdsm_frame(context: &GpuContext, independent: bool) -> Vec<Vec<u8>> {
    use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
    let source = source();
    let device = context.device();
    let mut scheduler = FrameRenderer::hdr(
        context,
        SIZE,
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    if independent {
        scheduler.independent_depth_for_test(device);
    }
    scheduler.set_frame_time(0.125).unwrap();
    let mut shadowed = crate::shadows::ShadowedScene::with_taa_velocity(
        context,
        &source,
        shadow_settings(),
        TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    shadowed
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: worldrunner_render_shadow::RangeFit {
                    near_step: 0.1,
                    far_step: 1.0,
                    minimum_span: 1.0,
                    minimum_span_policy: worldrunner_render_shadow::MinimumSpanPolicy::ExtendNear,
                    active_maximum: None,
                    split_lambda: 0.5,
                    split_policy: worldrunner_render_shadow::SplitPolicy::Practical,
                },
            }),
        )
        .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        scheduler.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        scheduler.targets().depth().texture(),
    )
    .unwrap();
    assert_eq!(
        scheduler.ssao.as_ref().unwrap().depth == *depth.texture(),
        !independent
    );
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: SIZE[0],
            height: SIZE[1],
        },
    )
    .unwrap();
    let fit = shadowed
        .render_depth_fitted_with(context, &view, (&color, &depth), 0, &mut scheduler)
        .unwrap()
        .unwrap();
    assert_eq!((fit.source_frame, fit.adopted_frame), (0, 0));
    assert_eq!(shadowed.last_depth_fit(), Some(fit));
    // Reading all products requires the suffix to have completed Opaque, snapshots and water.
    let products = frame_products(context, &scheduler, 0);
    assert!(
        products[1]
            .chunks_exact(4)
            .any(|pixel| pixel != 1.0_f32.to_le_bytes())
    );
    products
}

#[test]
fn gpu_same_frame_sdsm_shared_depth_completes_opaque_snapshots_and_water_suffix() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    assert_eq!(sdsm_frame(&context, true), sdsm_frame(&context, false));
}
