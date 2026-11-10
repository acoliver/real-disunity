use super::*;

type Capture = Vec<Vec<f32>>;

pub(super) fn source(case: Case, pair: bool) -> String {
    let sizes = policy(case).radiance_extents(SIZE);
    let mut source = gpu::configured_resolve_source(
        sizes.len(),
        case.samples,
        case.water,
        case.ao,
        (
            worldrunner_render_plan::SceneSsaoReceiver::Nearest,
            wgpu::TextureFormat::R32Float,
        ),
    );
    if case.full {
        source = pyramid::resolve_source(source, &sizes, case.samples);
    }
    if pair {
        source = source.replace(
            "return vec4<f32>(reflection_delta(reflection.rgb, reflection.a, factor, environment, replacement) * roughness_fade, 0.0);",
            "return reflection + vec4<f32>(reflection_delta(reflection.rgb, reflection.a, factor, environment, replacement) * roughness_fade * 0.0, 0.0);",
        );
    }
    source
}

pub(super) fn capture(context: &GpuContext, case: Case) -> (Capture, Capture) {
    capture_shaded(context, case, false)
}

pub(super) fn capture_shaded(
    context: &GpuContext,
    case: Case,
    perturbed: bool,
) -> (Capture, Capture) {
    let config = FrameConfig::new(SIZE, wgpu::TextureFormat::Rgba16Float, case.samples);
    let mut screen = SceneMetadata::new(context.device(), SIZE, neutral_ao()).unwrap();
    let ssr = Ssr::new(context.device(), config, policy(case), &mut screen).unwrap();
    let water = super::super::water_tests::water_fixture(context, config);
    initialize(context, &ssr, &water, case);
    if perturbed {
        let normal = [0.2, 0.0, 0.979_795_9, 1.0];
        upload(
            context,
            &ssr.normals,
            &normal.repeat(usize::try_from(SIZE[0] * SIZE[1]).unwrap()),
        );
        fill(
            context,
            &water.receivers[0][0],
            "vec4<f32>(0.2, 0.0, 0.9797959, 1.0)",
        );
    }
    let target = water.receivers[1][0].clone();
    commit(context, &ssr, &target);
    reduce(context, &ssr);
    let mut views = vec![
        view(&ssr.material),
        view(&ssr.normals),
        view(&ssr.source_depth),
        view(&ssr.hierarchy[0]),
    ];
    views.extend(ssr.reflection_views());
    views.extend(ssr.forward_products.iter().map(view));
    if case.water {
        let metres = match case.edge {
            Edge::Depth => "select(3.0, 4.0, p.x > 9.0)",
            Edge::None | Edge::Normal | Edge::Ownership => "3.0",
        };
        let depth = super::super::water_tests::raw_depth::texture(context, config, metres);
        water::Inputs::new(&water, 0).resolve_views(&mut views, view(&depth));
    }
    ssr.application_views(&mut views);
    let captures = [true, false].map(|pair| {
        let pipeline = gpu::pipeline(context.device(), &source(case, pair), case.samples, false);
        let group = gpu::group(context.device(), &pipeline, Some(&ssr.uniform), &views);
        render(context, &target, &pipeline, &[&group]);
        (0..case.samples)
            .map(|lane| read_lane(context, &target, lane))
            .collect()
    });
    let [pairs, deltas] = captures;
    (pairs, deltas)
}

fn view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn initialize(context: &GpuContext, ssr: &Ssr, water: &crate::water::Water, case: Case) {
    let count = usize::try_from(SIZE[0] * SIZE[1]).unwrap();
    let normals: Vec<_> = (0..SIZE[1])
        .flat_map(|_| {
            (0..SIZE[0]).flat_map(|x| {
                if matches!(case.edge, Edge::Normal) && x > SIZE[0] / 2 {
                    [1.0, 0.0, 0.0, 1.0]
                } else {
                    [0.0, 0.0, 1.0, 1.0]
                }
            })
        })
        .collect();
    upload(context, &ssr.normals, &normals);
    upload(
        context,
        &ssr.material,
        &[1.0, 1.0, 1.0, case.roughness].repeat(count),
    );
    upload(context, &ssr.source_depth, &opaque_depths(case));
    let depths: Vec<_> = (0..SIZE[1])
        .flat_map(|_| {
            (0..SIZE[0]).map(|x| {
                if matches!(case.edge, Edge::Depth) && x > SIZE[0] / 2 {
                    4.0
                } else {
                    3.0
                }
            })
        })
        .collect();
    upload(context, &ssr.hierarchy[0], &depths);
    upload(context, &ssr.application_ao, &vec![1.0; count]);
    fill(
        context,
        &ssr.forward_products[0],
        "vec4<f32>(2.0, 2.0, 2.0, 1.0)",
    );
    fill(
        context,
        &ssr.forward_products[1],
        "vec4<f32>(0.5, 0.5, 0.5, 1.0)",
    );
    initialize_water(context, water, case);
    upload_trace(context, ssr, case);
}

fn opaque_depths(case: Case) -> Vec<f32> {
    (0..SIZE[1])
        .flat_map(|_| {
            (0..SIZE[0]).map(|x| {
                // Coplanar opaque poison isolates ownership from depth rejection.
                if case.water && !(matches!(case.edge, Edge::Ownership) && x > SIZE[0] / 2) {
                    6.0
                } else {
                    3.0
                }
            })
        })
        .collect()
}

fn initialize_water(context: &GpuContext, water: &crate::water::Water, case: Case) {
    let normal = match case.edge {
        Edge::Ownership => "vec4<f32>(0.0, 0.0, 1.0, select(1.0, 0.0, p.x > 9.0))",
        Edge::Normal => {
            "select(vec4<f32>(0.0, 0.0, 1.0, 1.0), vec4<f32>(1.0, 0.0, 0.0, 1.0), p.x > 9.0)"
        }
        Edge::None | Edge::Depth => "vec4<f32>(0.0, 0.0, 1.0, 1.0)",
    };
    fill(context, &water.receivers[0][0], normal);
    fill(
        context,
        &water.receivers[0][1],
        &format!("vec4<f32>(0.5, 0.5, 1.0, {})", case.roughness),
    );
    fill(
        context,
        &water.receivers[0][2],
        "vec4<f32>(2.0, 2.0, 2.0, 1.0)",
    );
}

fn upload_trace(context: &GpuContext, ssr: &Ssr, case: Case) {
    let [width, height] = policy(case).radiance_extents(SIZE)[0];
    let grid = if case.full {
        pyramid::lane_grid(case.samples)
    } else {
        [case.samples, 1]
    };
    let texture = &ssr.reflections[0];
    let mut values = Vec::new();
    for y in 0..texture.height() {
        for x in 0..texture.width() {
            let lane = x / width + y / height * grid[0];
            let valid = x < width * grid[0] && y < height * grid[1];
            let hit = x % width == width / 2 && y % height == height / 2;
            let poison = !matches!(case.edge, Edge::None) && x % width > width / 2;
            let pair = if !valid {
                [99.0; 4]
            } else if poison {
                [8.0, 4.0, 2.0, 1.0]
            } else if hit {
                [0.25, 0.5, 0.75, 1.0].map(|v| v / gpu::scalar(lane + 1))
            } else {
                [0.0; 4]
            };
            values.extend(pair);
        }
    }
    upload(context, texture, &values);
}

fn commit(context: &GpuContext, ssr: &Ssr, target: &wgpu::Texture) {
    let mut feature = ReflectionFeature {
        water: None,
        ssr,
        frame: frame(),
        color: AttachmentView::new(ResourceRef::new("finite footprint target"), target).unwrap(),
        groups: Vec::new(),
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    encoder.finish().submit().unwrap();
}

fn reduce(context: &GpuContext, ssr: &Ssr) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    for index in 1..ssr.reflections.len() {
        let inputs = [ssr.reflection_view(index - 1)];
        let group = gpu::group(
            context.device(),
            &ssr.reduce,
            ssr.reduction_extents.get(index - 1),
            &inputs,
        );
        let target = ssr.reflection_view(index);
        let size = ssr.settings.radiance_extents(SIZE)[index];
        let grid = if pyramid::shared(&ssr.settings) {
            pyramid::lane_grid(ssr.capture_samples)
        } else {
            [ssr.capture_samples, 1]
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(attachment(&target))],
            ..Default::default()
        });
        pass.set_scissor_rect(0, 0, size[0] * grid[0], size[1] * grid[1]);
        pass.set_pipeline(&ssr.reduce);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
    encoder.finish().submit().unwrap();
}

fn attachment(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        resolve_target: None,
        depth_slice: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
        },
    }
}

fn render(
    context: &GpuContext,
    target: &wgpu::Texture,
    pipeline: &wgpu::RenderPipeline,
    groups: &[&wgpu::BindGroup],
) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("finite footprint reference"),
        attachment(&view(target)),
        pipeline,
        groups,
    );
    encoder.finish().submit().unwrap();
}

fn fill(context: &GpuContext, target: &wgpu::Texture, expression: &str) {
    let shader = format!(
        "@fragment fn fs_screen(@builtin(position) p: vec4<f32>, @builtin(sample_index) lane: u32) -> @location(0) vec4<f32> {{ return {expression}; }}"
    );
    let pipeline = gpu::pipeline(context.device(), &shader, target.sample_count(), false);
    render(context, target, &pipeline, &[]);
}

fn read_lane(context: &GpuContext, target: &wgpu::Texture, lane: u32) -> Vec<f32> {
    let (kind, lane) = if target.sample_count() == 1 {
        ("texture_2d<f32>", 0)
    } else {
        ("texture_multisampled_2d<f32>", lane)
    };
    let shader = format!(
        "@group(0) @binding(1) var image: {kind}; @fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {{ return textureLoad(image, vec2<i32>(p.xy), {lane}); }}"
    );
    let pipeline = gpu::pipeline(context.device(), &shader, 1, false);
    let group = gpu::group(context.device(), &pipeline, None, &[view(target)]);
    let output =
        crate::ssao::passes::texture(context.device(), SIZE, wgpu::TextureFormat::Rgba16Float);
    render(context, &output, &pipeline, &[&group]);
    read(context, &output)
        .chunks_exact(2)
        .map(|v| ::half::f16::from_le_bytes(v.try_into().unwrap()).to_f32())
        .collect()
}
