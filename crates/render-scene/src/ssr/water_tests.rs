#[test]
fn gpu_production_water_trace_and_resolve_share_registered_metadata() {
    let context = context();
    for samples in [1, 4] {
        let mut renderer = crate::compositor::FrameRenderer::hdr(
            &context,
            [65, 65],
            wgpu::TextureFormat::Rgba8UnormSrgb,
            samples,
            ToneMap::Reinhard,
            None,
        )
        .unwrap();
        renderer.ssao = Some(SceneMetadata::new(context.device(), [65, 65], neutral_ao()).unwrap());
        renderer.ssr = Some(
            Ssr::new(
                context.device(),
                renderer.targets().config(),
                settings(),
                renderer.ssao.as_mut().unwrap(),
            )
            .unwrap(),
        );
        renderer
            .install_water(
                context.device(),
                worldrunner_render_plan::SceneWaterBody {
                    material: worldrunner_render_plan::SceneWaterSettings::STOCK,
                    surface: worldrunner_render_plan::SceneWaterSurface::Rectangle,
                    flow_millimeters_per_second: [0; 2],
                    height_millimeters: -1000,
                    east_min_millimeters: -10000,
                    north_min_millimeters: -10000,
                    east_extent_millimeters: 20000,
                    north_extent_millimeters: 20000,
                    color_micro: [0; 3],
                    absorption_micro: [0; 3],
                    roughness_micro: 0,
                },
            )
            .unwrap();
        let (color, depth, mut opaque) = attachments(&renderer);
        let depth = depth
            .with_resource(ResourceRef::new("custom water depth"))
            .unwrap();
        opaque.depth.as_mut().unwrap().resource = depth.resource();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        renderer
            .encode_frame(
                context.device(),
                &mut encoder,
                (&[opaque], &[color, depth]),
                &mut [],
                (frame(), None),
            )
            .unwrap();
        encoder.finish().submit().unwrap();
        assert!(renderer.water_receivers().is_some());
        assert!(renderer.post_water_depth().is_some());
    }
}

use super::*;

fn clear_receivers(context: &GpuContext, water: &crate::water::Water) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let views = water.receivers[0]
        .each_ref()
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
    let colors = [
        wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 1.0,
            a: 1.0,
        },
        wgpu::Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.0,
        },
        wgpu::Color {
            r: 0.25,
            g: 0.125,
            b: 0.0625,
            a: 1.0,
        },
    ];
    let attachments: Vec<_> = views
        .iter()
        .zip(colors)
        .map(|(view, color)| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })
        })
        .collect();
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("water receiver fixture"),
        color_attachments: &attachments,
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
    encoder.finish().submit().unwrap();
}
pub(super) fn water_fixture(context: &GpuContext, config: FrameConfig) -> crate::water::Water {
    let mut water = crate::water::Water::new(context.device(), config, vec![]).unwrap();
    let lut =
        crate::ssao::passes::texture(context.device(), [1, 1], wgpu::TextureFormat::Rgba16Float);
    upload(context, &lut, &[0.5, 0.0, 0.0, 1.0]);
    water.environment[1] = lut.create_view(&wgpu::TextureViewDescriptor::default());
    clear_receivers(context, &water);
    water
}
fn water_resolve(context: &GpuContext, samples: u32, confidence: f32, partial: bool) -> Vec<u8> {
    let size = [3, 3];
    let config = FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, samples);
    let mut screen = SceneMetadata::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(context.device(), config, settings(), &mut screen).unwrap();
    let water = water_fixture(context, config);
    upload(context, &ssr.normals, &[0.0; 36]);
    if partial {
        select_last_sample(context, &water);
    }
    for texture in &ssr.reflections {
        upload(
            context,
            texture,
            &[confidence; 4].repeat(usize::try_from(texture.width() * texture.height()).unwrap()),
        );
    }
    let target = water.receivers[1][0].clone();
    let output =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    let inputs = water::Inputs::new(&water, 0);
    let mut feature = ReflectionFeature {
        water: Some(inputs),
        ssr: &ssr,
        frame: frame(),
        color: AttachmentView::new(ResourceRef::new("scene color"), &target).unwrap(),
        groups: vec![],
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    let mut views = vec![
        ssr.material
            .create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.normals
            .create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.hierarchy[0].create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.hierarchy[0].create_view(&wgpu::TextureViewDescriptor::default()),
    ];
    views.extend(
        ssr.reflections
            .iter()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    views.extend(
        ssr.forward_products
            .each_ref()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    feature.water.as_ref().unwrap().resolve_views(&mut views);
    let group = gpu::group(
        context.device(),
        &ssr.water_resolve,
        Some(&ssr.uniform),
        &views,
    );
    draw_water_resolve(&mut encoder, &ssr, &group, &target, &output);
    encoder.finish().submit().unwrap();
    read(context, &output)
}
fn draw_water_resolve(
    encoder: &mut wgpu::CommandEncoder,
    ssr: &Ssr,
    group: &wgpu::BindGroup,
    target: &wgpu::Texture,
    output: &wgpu::Texture,
) {
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    worldrunner_render_post::fullscreen_pass(
        encoder,
        Some("water replacement fixture"),
        wgpu::RenderPassColorAttachment {
            view: if target.sample_count() == 1 {
                &output_view
            } else {
                &view
            },
            depth_slice: None,
            resolve_target: (target.sample_count() != 1).then_some(&output_view),
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.5,
                    g: 0.375,
                    b: 0.25,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        },
        &ssr.water_resolve,
        &[group],
    );
}
#[test]
fn gpu_water_selection_and_replacement_are_exact_at_one_and_four_samples() {
    let context = context();
    for samples in [1, 4] {
        for (confidence, expected) in [
            (0.0, [0.5, 0.375, 0.25, 1.0]),
            (0.5, [0.625, 0.5625, 0.46875, 1.0]),
            (1.0, [0.75, 0.75, 0.6875, 1.0]),
        ] {
            let result = water_resolve(&context, samples, confidence, false);
            let expected: Vec<_> = expected
                .into_iter()
                .flat_map(|v| half(v).to_le_bytes())
                .collect();
            assert_eq!(
                &result[32..40],
                expected,
                "sample count {samples}, confidence {confidence}"
            );
        }
    }
}

fn select_last_sample(context: &GpuContext, water: &crate::water::Water) {
    let source = "@fragment fn fs_screen(@builtin(sample_index) sample: u32) -> @location(0) vec4<f32> { return vec4<f32>(0.0, 0.0, 1.0, select(0.0, 1.0, sample == 3u)); }";
    let pipeline = gpu::pipeline(context.device(), source, 4, false);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("sample-independent water edge"),
        wgpu::RenderPassColorAttachment {
            view: &water.receivers[0][0].create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &pipeline,
        &[],
    );
    encoder.finish().submit().unwrap();
}
#[test]
fn gpu_partial_water_coverage_replaces_only_its_own_sample() {
    let context = context();
    let result = water_resolve(&context, 4, 1.0, true);
    let expected: Vec<_> = [0.5625, 0.46875, 0.359_375, 1.0]
        .into_iter()
        .flat_map(|v| half(v).to_le_bytes())
        .collect();
    assert_eq!(&result[32..40], expected);
}

// Replays the production replacement pipeline with unit incident radiance.
// The caller has already captured its rendered water lobe and direct contribution.
pub(crate) fn replay_water_factor(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
    frame: &crate::frame::FrameUniform,
) -> Vec<u8> {
    let ssr = renderer.ssr.as_ref().unwrap();
    let water = renderer.water.as_ref().unwrap();
    for texture in &ssr.reflections {
        upload(
            context,
            texture,
            &[1.0; 4].repeat(usize::try_from(texture.width() * texture.height()).unwrap()),
        );
    }
    let views = replay_views(ssr, water);
    let group = gpu::group(
        context.device(),
        &ssr.water_resolve,
        Some(&ssr.uniform),
        &views,
    );
    let output =
        crate::ssao::passes::texture(context.device(), [32, 32], wgpu::TextureFormat::Rgba16Float);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut feature = ReflectionFeature {
        ssr,
        water: None,
        frame: *frame,
        color: AttachmentView::new(ResourceRef::new("replay color"), &output).unwrap(),
        groups: Vec::new(),
    };
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        wgpu::RenderPassColorAttachment {
            view: &output.create_view(&wgpu::TextureViewDescriptor::default()),
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                store: wgpu::StoreOp::Store,
            },
        },
        &ssr.water_resolve,
        &[&group],
    );
    encoder.finish().submit().unwrap();
    read(context, &output)
}

fn replay_views(ssr: &Ssr, water: &crate::water::Water) -> Vec<wgpu::TextureView> {
    let mut views = [
        &ssr.material,
        &ssr.normals,
        &ssr.hierarchy[0],
        &ssr.hierarchy[0],
    ]
    .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
    .to_vec();
    views.extend(
        ssr.reflections
            .iter()
            .chain(ssr.forward_products.iter())
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    water::Inputs::new(water, 0).resolve_views(&mut views);
    views
}
