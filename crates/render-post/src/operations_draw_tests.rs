use super::operations_tests::{device, id, underlying};
use crate::{AttachmentLoad, contracts::*};

const COMPUTE: &str = "@group(0) @binding(0) var output_image: texture_storage_2d<rgba8unorm, write>; @compute @workgroup_size(1) fn main(@builtin(global_invocation_id) p: vec3<u32>) { textureStore(output_image, vec2<i32>(p.xy), vec4<f32>(0., 1., 0., 1.)); }";
const DRAW: &str = "@group(0) @binding(0) var input: texture_2d<f32>; @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> { let p = array(vec2<f32>(-1., -1.), vec2<f32>(3., -1.), vec2<f32>(-1., 3.)); return vec4<f32>(p[i], 0., 1.); } @fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> { return textureLoad(input, vec2<i32>(p.xy), 0); }";
fn texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("compute draw fixture"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn group(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    view: CheckedView,
    storage: bool,
) -> CheckedBindings {
    CheckedBindings::build(
        device,
        registry,
        &[TextureBinding {
            binding: 0,
            visibility: if storage {
                wgpu::ShaderStages::COMPUTE
            } else {
                wgpu::ShaderStages::FRAGMENT
            },
            view,
            samples: 1,
            semantic: Semantic::Unspecified,
            kind: if storage {
                TextureBindingKind::Storage(wgpu::StorageTextureAccess::WriteOnly)
            } else {
                TextureBindingKind::Sampled(wgpu::TextureSampleType::Float { filterable: false })
            },
        }],
    )
    .unwrap()
}
fn producer(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    view: CheckedView,
) -> TypedOperation {
    let bindings = group(device, registry, view.clone(), true);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(bindings.layout())],
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(COMPUTE.into()),
    });
    let pipeline = crate::compute_pipeline(
        device,
        crate::ComputeDescriptor {
            label: None,
            layout: &layout,
            module: &module,
            entry: "main",
            constants: &[],
        },
    );
    let mut operation = TypedOperation::new(
        id("texture producer"),
        TypedWork::Compute {
            pipeline,
            bindings: vec![bindings],
            workgroups: [2, 2, 1],
        },
    );
    operation
        .textures
        .push(ResourceUse::write(view, UseKind::Storage));
    operation
}
fn consumer(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    source: CheckedView,
    target: CheckedView,
) -> TypedOperation {
    let bindings = group(device, registry, source.clone(), false);
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(bindings.layout())],
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(DRAW.into()),
    });
    let pipeline = crate::fullscreen_pipeline(
        device,
        crate::FullscreenDescriptor {
            label: None,
            layout: &layout,
            module: &module,
            vertex_entry: "vs",
            fragment_entry: "fs",
            format: wgpu::TextureFormat::Rgba8Unorm,
            constants: &[],
        },
    );
    let pass = RasterPass {
        id: id("texture consumer"),
        colors: vec![Some(ColorAttachment {
            view: target,
            before: None,
            load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
            resolve: None,
        })],
        depth: None,
        uses: vec![ResourceUse::read(source, UseKind::Sampled)],
        bindings: vec![bindings],
    };
    TypedOperation::new(pass.id, TypedWork::Fullscreen { pass, pipeline })
}
#[test]
fn compute_to_fullscreen_requires_declared_producer_and_preserves_bytes() {
    let (device, queue) = device();
    let mut registry = ResourceRegistry::default();
    let output = texture(&device);
    let input_key = registry
        .import(
            &texture(&device),
            ResourceLifetime::FrameLocal,
            Semantic::Unspecified,
            None,
        )
        .unwrap();
    let output_key = registry
        .import(
            &output,
            ResourceLifetime::FrameLocal,
            Semantic::Unspecified,
            None,
        )
        .unwrap();
    let range = TextureRange::single(0, 0, wgpu::TextureAspect::All);
    let input = registry.view(input_key, range.clone(), 1).unwrap();
    let target = registry.view(output_key, range, 1).unwrap();
    let first = producer(&device, &registry, input.clone());
    let second = consumer(&device, &registry, input, target);
    assert!(matches!(
        underlying(
            registry
                .compile_operations(&device, &[second.clone(), first.clone()])
                .err()
                .unwrap()
        ),
        ContractError::Producer(_)
    ));
    let compiled = registry
        .compile_operations(&device, &[first, second])
        .unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, None, |_, _| {})
        .unwrap();
    queue.submit([encoder.finish()]);
    let mut readback = crate::Readback::new(
        &device,
        crate::FrameConfig::new([2, 2], wgpu::TextureFormat::Rgba8Unorm, 1),
        512,
    )
    .unwrap();
    assert_eq!(
        readback.read(&device, &queue, &output).unwrap(),
        [0, 255, 0, 255].repeat(4)
    );
}
