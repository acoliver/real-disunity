use super::operations_tests::{buffer, device, id, underlying, upload};
use crate::contracts::*;

fn image(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("transfer fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 3,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
fn transfer_pair(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
) -> (Vec<TypedOperation>, wgpu::Buffer) {
    let source = registry
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
    let output = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let view = transfer_view(device, registry);
    let input = registry.buffer_view(source, 0..1024, 1).unwrap();
    let operations = vec![
        upload(input.clone(), &[29; 1024]),
        TypedOperation::new(
            id("buffer to layered texture"),
            TypedWork::CopyBufferToTexture {
                source: input,
                destination: TextureCopy::whole(view.clone()),
                bytes_per_row: 256,
                rows_per_image: 2,
            },
        ),
        TypedOperation::new(
            id("layered texture to buffer"),
            TypedWork::CopyTextureToBuffer {
                source: TextureCopy::whole(view),
                destination: registry.buffer_view(output, 0..1024, 1).unwrap(),
                bytes_per_row: 256,
                rows_per_image: 2,
            },
        ),
    ];
    (operations, staging)
}
#[test]
fn buffer_texture_transfers_preserve_selected_mip_and_array_layers() {
    let (device, queue) = device();
    let mut registry = ResourceRegistry::default();
    let (operations, staging) = transfer_pair(&device, &mut registry);
    let compiled = registry.compile_operations(&device, &operations).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, None, |_, _| {})
        .unwrap();
    queue.submit([encoder.finish()]);
    let mut pending = AsyncReadback::begin(&staging).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let bytes = loop {
        device.poll(wgpu::PollType::Poll).unwrap();
        if let Some(bytes) = pending.try_read().unwrap() {
            break bytes;
        }
        assert!(std::time::Instant::now() < deadline);
    };
    for row in [0, 256, 512, 768] {
        assert_eq!(&bytes[row..row + 8], &[29; 8]);
    }
}
#[test]
fn invalid_copy_capacity_extents_and_phantom_declarations_are_typed() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let (operations, _) = transfer_pair(&device, &mut registry);
    for mode in 0..3 {
        let mut invalid = operations.clone();
        if let TypedWork::CopyBufferToTexture {
            destination,
            rows_per_image,
            ..
        } = &mut invalid[1].work
        {
            match mode {
                0 => *rows_per_image = 1,
                1 => destination.origin = [u32::MAX, 0],
                _ => destination.extent = [3, 2],
            }
        }
        assert!(matches!(
            underlying(
                registry
                    .compile_operations(&device, &invalid)
                    .err()
                    .unwrap()
            ),
            ContractError::CopyExtent(_)
        ));
    }
    let mut phantom = operations;
    if let TypedWork::CopyBufferToTexture { source, .. } = &phantom[1].work {
        let source = source.clone();
        phantom[0]
            .buffers
            .push(BufferUse::read(source, BufferUseKind::Storage));
    }
    assert!(matches!(
        underlying(
            registry
                .compile_operations(&device, &phantom)
                .err()
                .unwrap()
        ),
        ContractError::Binding(_)
    ));
}
#[test]
fn read_only_buffers_and_replaced_compiled_generations_fail_before_encoding() {
    let (device, _queue) = device();
    let mut registry = ResourceRegistry::default();
    let usage = wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
    let immutable = registry
        .import_buffer(
            &buffer(&device, 8, usage),
            ResourceLifetime::ImportedReadOnly,
            Some(0),
        )
        .unwrap();
    let write = upload(registry.buffer_view(immutable, 0..8, 1).unwrap(), &[4; 8]);
    assert!(matches!(
        underlying(
            registry
                .compile_operations(&device, &[write])
                .err()
                .unwrap()
        ),
        ContractError::ReadOnly(_)
    ));
    assert!(matches!(
        registry.replace_buffer(immutable, &buffer(&device, 8, usage)),
        Err(ContractError::ReadOnlyReplacement(_))
    ));
    let mutable = registry
        .import_buffer(
            &buffer(&device, 8, usage),
            ResourceLifetime::FrameLocal,
            None,
        )
        .unwrap();
    let compiled = registry
        .compile_operations(
            &device,
            &[upload(
                registry.buffer_view(mutable, 0..8, 1).unwrap(),
                &[4; 8],
            )],
        )
        .unwrap();
    registry
        .replace_buffer(mutable, &buffer(&device, 8, usage))
        .unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    assert!(matches!(
        compiled.encode(&registry, &mut encoder, None, |_, _| panic!(
            "must not draw"
        )),
        Err(ContractError::StaleAllocation(_))
    ));
}

fn transfer_view(device: &wgpu::Device, registry: &mut ResourceRegistry) -> CheckedView {
    let texture = registry
        .import(
            &image(device),
            ResourceLifetime::FrameLocal,
            Semantic::Unspecified,
            None,
        )
        .unwrap();
    registry
        .view(
            texture,
            TextureRange {
                mips: 1..2,
                layers: 1..3,
                aspect: wgpu::TextureAspect::All,
            },
            1,
        )
        .unwrap()
}
