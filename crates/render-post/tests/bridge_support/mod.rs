use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_gpu::{device::DeviceRequirements, test_support::TestGpu};
use worldrunner_render_post::{AttachmentLoad, FrameConfig, Readback, StageId, contracts::*};

pub fn gpu() -> (wgpu::Device, SubmissionAuthority) {
    // Each fixture owns its authority and resources; callbacks remain the cache's fail-fast ones.
    let gpu = TestGpu::shared(&DeviceRequirements::default()).unwrap();
    (gpu.device, SubmissionAuthority::new(gpu.queue))
}

#[test]
fn ordinary_bridge_fixtures_share_handles_but_keep_authorities_local() {
    let (first, first_authority) = gpu();
    let (second, second_authority) = gpu();
    assert_eq!(first, second);
    assert_eq!(first_authority.queue(), second_authority.queue());
    assert!(first.features().is_empty());
    assert_eq!(first_authority.submitted_serial(), 0);
    assert_eq!(second_authority.submitted_serial(), 0);
    let frame = first_authority.begin(&first).unwrap();
    assert!(!first_authority.is_idle());
    assert!(second_authority.is_idle());
    frame.finish().cancel();
}

pub fn id(name: &'static str) -> RasterId {
    RasterId::new(StageId::new(name, 0), 0)
}
pub fn texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    samples: u32,
    layers: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("checked bridge fixture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | if samples == 1 {
                wgpu::TextureUsages::COPY_SRC
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    })
}
pub fn view(
    registry: &mut ResourceRegistry,
    texture: &wgpu::Texture,
    layer: u32,
    version: u32,
) -> CheckedView {
    let key = registry
        .import(
            texture,
            ResourceLifetime::FrameLocal,
            if texture.format().has_depth_aspect() {
                Semantic::DeviceDepth
            } else {
                Semantic::LinearColor
            },
            None,
        )
        .unwrap();
    registry
        .view(
            key,
            TextureRange::single(0, layer, wgpu::TextureAspect::All),
            version,
        )
        .unwrap()
}
pub fn pass(name: &'static str, target: CheckedView) -> RasterPass {
    RasterPass {
        id: id(name),
        colors: vec![Some(ColorAttachment {
            view: target,
            before: None,
            load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
            resolve: None,
        })],
        depth: None,
        uses: vec![],
        bindings: vec![],
    }
}
pub fn read(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    Readback::new(device, FrameConfig::new([1, 1], texture.format(), 1), 256)
        .unwrap()
        .read(device, authority, texture)
        .unwrap()
}
pub const VERTEX: &str = "@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> { let p = array(vec2<f32>(-1., -1.), vec2<f32>(3., -1.), vec2<f32>(-1., 3.)); return vec4<f32>(p[i], 0., 1.); }";
pub fn draw(
    device: &wgpu::Device,
    shader: &str,
    groups: Vec<CheckedBindings>,
    samples: u32,
) -> CheckedDraw {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    CheckedDraw::new(device, &descriptor(&module, samples), groups, 0..3, 0..1).unwrap()
}
pub fn descriptor(module: &wgpu::ShaderModule, samples: u32) -> wgpu::RenderPipelineDescriptor<'_> {
    wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: samples,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    }
}
pub fn execute(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    registry: &ResourceRegistry,
    ops: &[TypedOperation],
) {
    let plan = registry.compile_operations(device, ops).unwrap();
    authority
        .prepare(device, |encoder, _| {
            plan.encode_checked(registry, encoder, None)
        })
        .unwrap()
        .submit()
        .unwrap();
}
