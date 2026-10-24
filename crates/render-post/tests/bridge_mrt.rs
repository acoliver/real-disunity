use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_post::{
    AttachmentLoad, FrameConfig, Readback, StageId,
    contracts::{
        CheckedDraw, ColorAttachment, DepthAttachment, RasterId, RasterPass, ResourceLifetime,
        ResourceRegistry, Semantic, TextureRange, TypedOperation, TypedWork,
    },
};

fn texture(device: &wgpu::Device, depth: bool) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bridge MRT"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if depth {
            wgpu::TextureFormat::Depth32Float
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | if depth {
                wgpu::TextureUsages::empty()
            } else {
                wgpu::TextureUsages::COPY_SRC
            },
        view_formats: &[],
    })
}
fn draw(device: &wgpu::Device, near: bool) -> CheckedDraw {
    let (depth, color) = if near {
        ("0.25", "vec4<f32>(1., 0., 0., 1.)")
    } else {
        ("0.75", "vec4<f32>(0., 1., 0., 1.)")
    };
    let shader = format!(
        "struct Output {{ @location(0) a: vec4<f32>, @location(1) b: vec4<f32>, @location(2) c: vec4<f32> }} @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{ let p = array(vec2<f32>(-1., -1.), vec2<f32>(3., -1.), vec2<f32>(-1., 3.)); return vec4<f32>(p[i], {depth}, 1.); }} @fragment fn fs() -> Output {{ return Output({color}, vec4<f32>(0., 0., 1., 1.), vec4<f32>(1.)); }}"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    let targets = vec![
        Some(wgpu::ColorTargetState {
            format: wgpu::TextureFormat::Rgba8Unorm,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL
        });
        3
    ];
    CheckedDraw::new(
        device,
        &wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        },
        vec![],
        0..3,
        0..1,
    )
    .unwrap()
}
#[test]
fn checked_providers_share_three_targets_and_depth() {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let authority = SubmissionAuthority::new(queue);
    for reverse in [false, true] {
        prove_depth(&device, &authority, reverse);
    }
}
fn prove_depth(device: &wgpu::Device, authority: &SubmissionAuthority, reverse: bool) {
    let mut registry = ResourceRegistry::default();
    let textures: Vec<_> = (0..4).map(|i| texture(device, i == 3)).collect();
    let views: Vec<_> = textures
        .iter()
        .map(|texture| {
            let key = registry
                .import(
                    texture,
                    ResourceLifetime::FrameLocal,
                    Semantic::Unspecified,
                    None,
                )
                .unwrap();
            registry
                .view(key, TextureRange::single(0, 0, wgpu::TextureAspect::All), 1)
                .unwrap()
        })
        .collect();
    let pass = attachments(&views);
    let id = pass.id;
    let mut draws = vec![draw(device, false), draw(device, true)];
    if reverse {
        draws.reverse();
    }
    let plan = registry
        .compile_operations(
            device,
            &[TypedOperation::new(id, TypedWork::Draws { pass, draws })],
        )
        .unwrap();
    authority
        .prepare(device, |encoder, _| {
            plan.encode_checked(&registry, encoder, None)
        })
        .unwrap()
        .submit()
        .unwrap();
    for (texture, expected) in
        textures[..3]
            .iter()
            .zip([[255, 0, 0, 255], [0, 0, 255, 255], [255; 4]])
    {
        let bytes = Readback::new(
            device,
            FrameConfig::new([1, 1], wgpu::TextureFormat::Rgba8Unorm, 1),
            256,
        )
        .unwrap()
        .read(device, authority, texture)
        .unwrap();
        assert_eq!(bytes, expected);
    }
}

fn attachments(views: &[worldrunner_render_post::contracts::CheckedView]) -> RasterPass {
    let id = RasterId::new(StageId::new("three target providers", 0), 0);
    RasterPass {
        id,
        colors: views[..3]
            .iter()
            .map(|view| {
                Some(ColorAttachment {
                    view: view.clone(),
                    before: None,
                    load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                    resolve: None,
                })
            })
            .collect(),
        depth: Some(DepthAttachment {
            view: views[3].clone(),
            before: None,
            load: AttachmentLoad::ClearDepth(1.),
            store: wgpu::StoreOp::Store,
            read_only: false,
        }),
        uses: vec![],
        bindings: vec![],
    }
}
