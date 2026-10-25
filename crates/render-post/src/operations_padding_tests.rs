use super::operations_tests::{buffer, device, id, underlying, upload};
use crate::contracts::*;

#[test]
fn transfer_versions_cover_rows_not_padding_image_gaps_or_tail() {
    let (device, queue) = device();
    let mut registry = ResourceRegistry::default();
    let (input, output, view) = fixture(&device, &mut registry);
    let mut ops = transfers(&registry, input, output, view);
    let staging = buffer(
        &device,
        16,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let target = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    check_versions(&device, &registry, &ops, output, target);
    for (index, version) in [2, 1].into_iter().enumerate() {
        let offset = index as u64 * 8;
        ops.push(TypedOperation::new(
            RasterId::new(
                crate::StageId::new("verify row and padding", 0),
                u32::try_from(index).unwrap(),
            ),
            TypedWork::ReadbackBuffer {
                source: registry
                    .buffer_view(output, offset..offset + 8, version)
                    .unwrap(),
                destination: registry.buffer_view(target, offset..offset + 8, 1).unwrap(),
            },
        ));
    }
    let compiled = registry.compile_operations(&device, &ops).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, None, |_, _| {})
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = read(&device, &staging);
    assert_eq!(bytes, [vec![29; 8], vec![73; 8]].concat());
}

pub(super) fn read(device: &wgpu::Device, staging: &wgpu::Buffer) -> Vec<u8> {
    let mut pending = AsyncReadback::begin(staging).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        device.poll(wgpu::PollType::Poll).unwrap();
        if let Some(bytes) = pending.try_read().unwrap() {
            return bytes;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "GPU readback did not complete"
        );
        std::thread::yield_now();
    }
}

fn fixture(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
) -> (AllocationKey, AllocationKey, CheckedView) {
    let usage = wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
    let input = registry
        .import_buffer(
            &buffer(device, 1536, usage),
            ResourceLifetime::FrameLocal,
            None,
        )
        .unwrap();
    let backing = buffer(device, 1536, usage);
    let output = registry
        .import_buffer(&backing, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let image = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("padded transfer"),
        size: wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 2,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texture = registry
        .import(
            &image,
            ResourceLifetime::FrameLocal,
            Semantic::Unspecified,
            None,
        )
        .unwrap();
    let view = registry
        .view(
            texture,
            TextureRange {
                mips: 0..1,
                layers: 0..2,
                aspect: wgpu::TextureAspect::All,
            },
            1,
        )
        .unwrap();
    (input, output, view)
}
fn transfers(
    registry: &ResourceRegistry,
    input: AllocationKey,
    output: AllocationKey,
    view: CheckedView,
) -> Vec<TypedOperation> {
    let mut ops = Vec::new();
    for (index, offset) in [0, 256, 768, 1024].into_iter().enumerate() {
        let mut op = upload(
            registry.buffer_view(input, offset..offset + 8, 1).unwrap(),
            &[29; 8],
        );
        op.id.occurrence = u32::try_from(index).unwrap();
        ops.push(op);
    }
    let mut padding = upload(registry.buffer_view(output, 8..16, 1).unwrap(), &[73; 8]);
    padding.id = id("padding upload");
    ops.push(padding);
    ops.push(TypedOperation::new(
        id("row upload"),
        TypedWork::CopyBufferToTexture {
            source: registry.buffer_view(input, 0..1536, 1).unwrap(),
            destination: TextureCopy::whole(view.clone()),
            bytes_per_row: 256,
            rows_per_image: 3,
        },
    ));
    ops.push(TypedOperation::new(
        id("row download"),
        TypedWork::CopyTextureToBuffer {
            source: TextureCopy::whole(view),
            destination: registry.buffer_view(output, 0..1536, 2).unwrap(),
            bytes_per_row: 256,
            rows_per_image: 3,
        },
    ));
    ops
}
fn check_versions(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    ops: &[TypedOperation],
    output: AllocationKey,
    target: AllocationKey,
) {
    for (offset, version, valid) in [
        (0, 2, true),
        (0, 1, false),
        (8, 1, true),
        (8, 2, false),
        (512, 2, false),
        (1280, 2, false),
    ] {
        let mut probe = ops.to_vec();
        probe.push(TypedOperation::new(
            id("probe"),
            TypedWork::CopyBuffer {
                source: registry
                    .buffer_view(output, offset..offset + 8, version)
                    .unwrap(),
                destination: registry.buffer_view(target, 0..8, 1).unwrap(),
            },
        ));
        let result = registry.compile_operations(device, &probe);
        if valid {
            assert!(result.is_ok());
        } else {
            assert!(matches!(
                underlying(result.err().unwrap()),
                ContractError::Producer(_)
            ));
        }
    }
}
