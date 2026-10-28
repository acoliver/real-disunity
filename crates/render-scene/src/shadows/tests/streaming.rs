use super::*;

fn multilevel_source() -> RenderScene {
    let source = super::terrain::terrain_scene(&scene(&[], 600_000, None), false, false);
    let mut heights = vec![0; 81];
    heights[20] = 3_000;
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 9,
        rows: 9,
        origin_east_millimeters: -4_000,
        origin_north_millimeters: -4_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: heights,
        surface_indices: vec![0; 81],
        surfaces: source.terrain().unwrap().surfaces().to_vec(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap()));
    source
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}

fn observe_preparations(render: impl FnOnce()) -> Vec<crate::draw::StreamingPreparation> {
    crate::draw::STREAMING_PREPARATIONS.with_borrow_mut(|trace| *trace = Some(Vec::new()));
    render();
    crate::draw::STREAMING_PREPARATIONS.with_borrow_mut(|trace| trace.take().unwrap())
}

fn assert_one_selection(preparations: &[crate::draw::StreamingPreparation], budget: usize) {
    let first = &preparations[0];
    assert_eq!(first.buffers.len(), 4);
    assert_eq!(first.uploads, 4);
    for preparation in &preparations[1..] {
        assert_eq!(preparation.buffers, first.buffers);
        assert_eq!(preparation.uploads, 0);
    }
    assert!(
        preparations
            .iter()
            .map(|entry| entry.uploads)
            .sum::<usize>()
            <= budget
    );
}

#[test]
fn multilevel_same_frame_capture_reuses_prefix_chunks_and_upload_budget() {
    use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
    let context = context();
    let source = multilevel_source();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    renderer
        .set_depth_fitting(
            &context,
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
    let preparations = observe_preparations(|| {
        let fit = renderer
            .render_depth_fitted(
                &context,
                &view(),
                (&target.color_attachment(), &target.depth_attachment()),
                0,
            )
            .unwrap()
            .unwrap();
        assert_eq!((fit.source_frame, fit.adopted_frame), (0, 0));
    });
    let budget = usize::try_from(
        source
            .terrain()
            .unwrap()
            .streaming_policy()
            .unwrap()
            .uploads_per_frame(),
    )
    .unwrap();
    assert!(
        preparations.len() >= 2,
        "prefix and suffix must both prepare geometry"
    );
    assert_one_selection(&preparations, budget);
    renderer.set_depth_fitting(&context, None).unwrap();
    let next =
        observe_preparations(|| render_target(&mut renderer, &context, &view(), &target).unwrap());
    assert_eq!(
        next[0].buffers.len(),
        7,
        "the terrain still needs another split"
    );
}

#[test]
fn ordinary_shadowed_frame_refines_multilevel_terrain_once_per_frame() {
    let context = context();
    let source = multilevel_source();
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let first =
        observe_preparations(|| render_target(&mut renderer, &context, &view(), &target).unwrap());
    assert_eq!(first.len(), 1);
    assert_one_selection(&first, 4);
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    let next =
        observe_preparations(|| render_target(&mut renderer, &context, &view(), &target).unwrap());
    assert_eq!(next[0].buffers.len(), 7);
    assert_eq!(next[0].uploads, 4);
}

#[test]
fn streamed_shadow_depth_matches_forward_surface_and_default_shadow() {
    let context = context();
    let source =
        super::terrain::terrain_scene(&scene(&[[0, 0, 1000]], 600_000, None), false, false);
    let streamed = source
        .with_terrain(Some(std::sync::Arc::new(
            source
                .terrain()
                .unwrap()
                .clone()
                .with_streaming_policy(Some(TerrainStreamingPolicy::new(8, 9, 4, 1000).unwrap())),
        )))
        .unwrap();
    let expected = capture_scene_shadowed(&context, &source, &view(), settings()).unwrap();
    let actual = capture_scene_shadowed(&context, &streamed, &view(), settings()).unwrap();
    assert_eq!(
        at(&actual, Vec3::new(-0.9, 0.0, 0.0)),
        at(&expected, Vec3::new(-0.9, 0.0, 0.0))
    );
    let baseline = capture_scene(&context, &streamed, &view()).unwrap();
    assert_shadow(&actual, &baseline, Vec3::new(-0.9, 0.0, 0.0));
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
}

fn screen_source() -> RenderScene {
    let base = super::terrain::terrain_scene(&scene(&[], 600_000, None), true, false);
    let mut parts = fixture::effects_parts(600_000);
    parts.ssao = Some(SceneSsao {
        depth_footprint: worldrunner_render_plan::SceneSsaoDepthFootprint::default(),
        kernel: SceneSsaoKernel::Hemisphere,
        filter: SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 1000,
        intensity_micro: 1_000_000,
        sample_count: 8,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    });
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_ssr(SceneSsr::default())
        .unwrap();
    let terrain = base
        .terrain()
        .unwrap()
        .clone()
        .with_streaming_policy(Some(TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap()));
    base.with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
        .with_objects_and_effects(vec![], Some(effects))
        .unwrap()
}

#[test]
fn streamed_ssao_ssr_prepass_depth_equals_forward_depth_in_shadowed_frame() {
    let context = context();
    let source = screen_source();
    let size = [view().viewport().width, view().viewport().height];
    let mut resident = ShadowedScene::with_target(
        &context,
        &source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let mut frame = crate::compositor::FrameRenderer::hdr(
        &context,
        size,
        OffscreenTarget::COLOR_FORMAT,
        1,
        worldrunner_render_post::ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let depth =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Depth32Float);
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let attachment = AttachmentView::new(ResourceRef::new("scene depth"), &depth).unwrap();
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            resident.encode_with(
                &context,
                &view(),
                (&color, &attachment),
                encoder,
                Some(&mut frame),
            )
        })
        .unwrap();
    prepared.submit().unwrap();
    assert_eq!(
        depth_bytes(&context, &depth),
        depth_bytes(&context, &frame.ssao.as_ref().unwrap().depth)
    );
    assert!(frame.ssao.as_ref().unwrap().geometry.reflection.is_some());
}

fn depth_bytes(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    let pitch = (texture.width() * 4).next_multiple_of(256);
    let size = u64::from(pitch) * u64::from(texture.height());
    let buffer = context.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("streamed depth comparison"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let mut readback =
        worldrunner_render_post::BufferReadback::new(context.device(), size, size).unwrap();
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    aspect: wgpu::TextureAspect::DepthOnly,
                    ..texture.as_image_copy()
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(pitch),
                        rows_per_image: Some(texture.height()),
                    },
                },
                texture.size(),
            );
            readback.encode(encoder, &buffer)
        })
        .unwrap();
    let submission = prepared.submit().unwrap();
    readback
        .read_encoded(context.device(), Some(submission))
        .unwrap()
}
