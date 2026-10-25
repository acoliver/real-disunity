use super::operations_tests::{buffer, device, id, underlying, upload};
use crate::contracts::*;

fn texture(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    format: wgpu::TextureFormat,
    size: [u32; 3],
    mips: u32,
    initial: Option<u32>,
) -> AllocationKey {
    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("copy rule fixture"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: size[2],
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    registry
        .import(
            &image,
            ResourceLifetime::PersistentMutable,
            Semantic::Unspecified,
            initial,
        )
        .unwrap()
}
fn copy(
    registry: &ResourceRegistry,
    key: AllocationKey,
    mip: u32,
    aspect: wgpu::TextureAspect,
    version: u32,
) -> TextureCopy {
    TextureCopy::whole(
        registry
            .view(key, TextureRange::single(mip, 0, aspect), version)
            .unwrap(),
    )
}
fn submit(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    registry: &ResourceRegistry,
    ops: &[TypedOperation],
) {
    let compiled = registry.compile_operations(device, ops).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(registry, &mut encoder, None, |_, _| {})
        .unwrap();
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::Poll).unwrap();
}
#[test]
fn compressed_small_and_edge_mips_use_physical_extents_in_all_directions() {
    let (device, queue) = compressed_device();
    let mut registry = ResourceRegistry::default();
    let a = texture(
        &device,
        &mut registry,
        wgpu::TextureFormat::Bc1RgbaUnorm,
        [12, 12, 1],
        4,
        None,
    );
    let b = texture(
        &device,
        &mut registry,
        wgpu::TextureFormat::Bc1RgbaUnorm,
        [12, 12, 1],
        4,
        None,
    );
    let key = registry
        .import_buffer(
            &buffer(
                &device,
                512,
                wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            ),
            ResourceLifetime::FrameLocal,
            None,
        )
        .unwrap();
    let staging = buffer(
        &device,
        512,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let target = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    for (mip, physical) in [(1, 8), (2, 4), (3, 4)] {
        let source = copy(&registry, a, mip, wgpu::TextureAspect::All, 1);
        let destination = copy(&registry, b, mip, wgpu::TextureAspect::All, 1);
        assert_eq!(source.extent, [physical; 2]);
        let ops = compressed_operations(&registry, key, target, source, destination);
        let mut invalid = ops.clone();
        if let TypedWork::CopyBufferToTexture { destination, .. } = &mut invalid[1].work {
            destination.extent = [3, 3];
        }
        assert!(matches!(
            underlying(
                registry
                    .compile_operations(&device, &invalid)
                    .err()
                    .unwrap()
            ),
            ContractError::CopyAlignment(_)
        ));
        submit(&device, &queue, &registry, &ops);
        let bytes = super::operations_padding_tests::read(&device, &staging);
        let row_bytes = (physical / 4 * 8) as usize;
        assert_eq!(&bytes[..row_bytes], vec![37; row_bytes]);
    }
}
#[test]
fn depth_and_stencil_copy_rules_reject_before_encoding() {
    let (device, queue) = device();
    let mut registry = ResourceRegistry::default();
    let depth = texture(
        &device,
        &mut registry,
        wgpu::TextureFormat::Depth32Float,
        [4, 4, 1],
        1,
        Some(0),
    );
    let other = texture(
        &device,
        &mut registry,
        wgpu::TextureFormat::Depth32Float,
        [4, 4, 1],
        1,
        None,
    );
    let source = copy(&registry, depth, 0, wgpu::TextureAspect::DepthOnly, 0);
    let destination = copy(&registry, other, 0, wgpu::TextureAspect::DepthOnly, 1);
    let staging = buffer(
        &device,
        1024,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let target = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let read = TypedOperation::new(
        id("depth read"),
        TypedWork::ReadbackTexture {
            source: source.clone(),
            destination: registry.buffer_view(target, 0..1024, 1).unwrap(),
            bytes_per_row: 256,
            rows_per_image: 4,
        },
    );
    let transfer = TypedOperation::new(
        id("depth copy"),
        TypedWork::CopyTexture {
            source: source.clone(),
            destination: destination.clone(),
        },
    );
    submit(
        &device,
        &queue,
        &registry,
        &[transfer.clone(), read.clone()],
    );
    assert_eq!(
        super::operations_padding_tests::read(&device, &staging).len(),
        1024
    );
    reject_depth_copies(&device, &mut registry, read, transfer, destination);
    stencil_roundtrip(&device, &queue, &mut registry);
}
fn stencil_roundtrip(device: &wgpu::Device, queue: &wgpu::Queue, registry: &mut ResourceRegistry) {
    let stencil = texture(
        device,
        registry,
        wgpu::TextureFormat::Depth24PlusStencil8,
        [4, 4, 1],
        1,
        None,
    );
    let view = copy(registry, stencil, 0, wgpu::TextureAspect::StencilOnly, 1);
    let input = registry
        .import_buffer(
            &buffer(
                device,
                1024,
                wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            ),
            ResourceLifetime::FrameLocal,
            None,
        )
        .unwrap();
    let staging = buffer(
        device,
        1024,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let target = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let ops = [
        upload(
            registry.buffer_view(input, 0..1024, 1).unwrap(),
            &[19; 1024],
        ),
        TypedOperation::new(
            id("stencil upload"),
            TypedWork::CopyBufferToTexture {
                source: registry.buffer_view(input, 0..1024, 1).unwrap(),
                destination: view.clone(),
                bytes_per_row: 256,
                rows_per_image: 4,
            },
        ),
        TypedOperation::new(
            id("stencil read"),
            TypedWork::ReadbackTexture {
                source: view,
                destination: registry.buffer_view(target, 0..1024, 1).unwrap(),
                bytes_per_row: 256,
                rows_per_image: 4,
            },
        ),
    ];
    submit(device, queue, registry, &ops);
    let bytes = super::operations_padding_tests::read(device, &staging);
    for row in 0..4 {
        assert_eq!(&bytes[row * 256..row * 256 + 4], &[19; 4]);
    }
}
#[test]
fn cube_views_validate_backing_shape_even_at_square_final_mip() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    for size in [[4, 2, 6], [4, 4, 6]] {
        let key = texture(
            &device,
            &mut registry,
            wgpu::TextureFormat::Rgba8Unorm,
            size,
            3,
            Some(0),
        );
        for dimension in [
            wgpu::TextureViewDimension::Cube,
            wgpu::TextureViewDimension::CubeArray,
        ] {
            let result = registry.view_dimension(
                key,
                TextureRange {
                    mips: 2..3,
                    layers: 0..6,
                    aspect: wgpu::TextureAspect::All,
                },
                0,
                dimension,
            );
            if size[0] == size[1] {
                let view = result.unwrap();
                CheckedBindings::build(
                    &device,
                    &registry,
                    &[TextureBinding {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        view,
                        samples: 1,
                        semantic: Semantic::Unspecified,
                        kind: TextureBindingKind::Sampled(wgpu::TextureSampleType::Float {
                            filterable: false,
                        }),
                    }],
                )
                .unwrap();
            } else {
                assert!(matches!(result, Err(ContractError::Range(_))));
            }
        }
    }
}

fn compressed_operations(
    registry: &ResourceRegistry,
    key: AllocationKey,
    target: AllocationKey,
    source: TextureCopy,
    destination: TextureCopy,
) -> Vec<TypedOperation> {
    vec![
        upload(registry.buffer_view(key, 0..512, 1).unwrap(), &[37; 512]),
        TypedOperation::new(
            id("compressed upload"),
            TypedWork::CopyBufferToTexture {
                source: registry.buffer_view(key, 0..512, 1).unwrap(),
                destination: source.clone(),
                bytes_per_row: 256,
                rows_per_image: 2,
            },
        ),
        TypedOperation::new(
            id("compressed copy"),
            TypedWork::CopyTexture {
                source,
                destination: destination.clone(),
            },
        ),
        TypedOperation::new(
            id("compressed read"),
            TypedWork::ReadbackTexture {
                source: destination,
                destination: registry.buffer_view(target, 0..512, 1).unwrap(),
                bytes_per_row: 256,
                rows_per_image: 2,
            },
        ),
    ]
}

fn reject_depth_copies(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    read: TypedOperation,
    transfer: TypedOperation,
    destination: TextureCopy,
) {
    for mut invalid in [read, transfer] {
        match &mut invalid.work {
            TypedWork::ReadbackTexture { source, .. } | TypedWork::CopyTexture { source, .. } => {
                source.extent = [2, 4];
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            underlying(
                registry
                    .compile_operations(device, &[invalid])
                    .err()
                    .unwrap()
            ),
            ContractError::CopyExtent(_)
        ));
    }
    let input = registry
        .import_buffer(
            &buffer(device, 1024, wgpu::BufferUsages::COPY_SRC),
            ResourceLifetime::ImportedReadOnly,
            Some(0),
        )
        .unwrap();
    let forbidden = TypedOperation::new(
        id("forbidden depth upload"),
        TypedWork::CopyBufferToTexture {
            source: registry.buffer_view(input, 0..1024, 0).unwrap(),
            destination,
            bytes_per_row: 256,
            rows_per_image: 4,
        },
    );
    assert!(matches!(
        underlying(
            registry
                .compile_operations(device, &[forbidden])
                .err()
                .unwrap()
        ),
        ContractError::CopyDirection { read: false, .. }
    ));
}

fn compressed_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        ..Default::default()
    }))
    .unwrap()
}
