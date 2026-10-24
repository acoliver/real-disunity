use crate::{StageId, contracts::*};

pub(super) fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap()
}
pub(super) fn buffer(device: &wgpu::Device, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("operation fixture"),
        size,
        usage,
        mapped_at_creation: false,
    })
}
pub(super) fn id(name: &'static str) -> RasterId {
    RasterId::new(StageId::new(name, 0), 0)
}
pub(super) fn upload(view: BufferView, bytes: &[u8]) -> TypedOperation {
    TypedOperation::new(
        id("upload"),
        TypedWork::Upload {
            destination: view,
            bytes: bytes.to_vec(),
        },
    )
}
pub(super) fn underlying(error: ContractError) -> ContractError {
    match error {
        ContractError::Pass { source, .. } => *source,
        other => other,
    }
}

#[test]
fn buffer_identity_generation_alignment_and_disjoint_ranges() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let usage = wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
    let backing = buffer(&device, 32, usage);
    let key = registry
        .import_buffer(&backing, ResourceLifetime::FrameLocal, None)
        .unwrap();
    assert_eq!(
        key,
        registry
            .import_buffer(&backing.clone(), ResourceLifetime::FrameLocal, None)
            .unwrap()
    );
    let first = registry.buffer_view(key, 0..8, 1).unwrap();
    let second = registry.buffer_view(key, 8..16, 1).unwrap();
    let mut a = upload(first.clone(), &[1; 8]);
    let mut b = upload(second, &[2; 8]);
    b.id = id("second upload");
    assert!(
        registry
            .compile_operations(&device, &[a.clone(), b])
            .is_ok()
    );
    a.work = TypedWork::Upload {
        destination: registry.buffer_view(key, 1..9, 1).unwrap(),
        bytes: vec![0; 8],
    };
    assert!(matches!(
        underlying(registry.compile_operations(&device, &[a]).err().unwrap()),
        ContractError::CopyAlignment(_)
    ));
    let replacement = registry
        .replace_buffer(key, &buffer(&device, 32, usage))
        .unwrap();
    assert_eq!(replacement.generation(), key.generation() + 1);
    assert!(matches!(
        registry.check_buffer(&first),
        Err(ContractError::StaleAllocation(_))
    ));
    assert!(
        registry
            .import_buffer(&backing, ResourceLifetime::FrameLocal, None)
            .is_err()
    );
}

#[test]
fn buffer_hazards_and_stale_versions_are_typed() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import_buffer(
            &buffer(
                &device,
                32,
                wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            ),
            ResourceLifetime::PersistentMutable,
            Some(0),
        )
        .unwrap();
    let source = registry.buffer_view(key, 0..8, 0).unwrap();
    let destination = registry.buffer_view(key, 4..12, 1).unwrap();
    let copy = TypedOperation::new(
        id("overlap"),
        TypedWork::CopyBuffer {
            source: source.clone(),
            destination,
        },
    );
    assert!(matches!(
        underlying(registry.compile_operations(&device, &[copy]).err().unwrap()),
        ContractError::Hazard(_)
    ));
    let other = registry
        .import_buffer(
            &buffer(&device, 32, wgpu::BufferUsages::COPY_DST),
            ResourceLifetime::FrameLocal,
            None,
        )
        .unwrap();
    let read = TypedOperation::new(
        id("stale copy"),
        TypedWork::CopyBuffer {
            source,
            destination: registry.buffer_view(other, 16..24, 1).unwrap(),
        },
    );
    let write = upload(registry.buffer_view(key, 0..8, 1).unwrap(), &[3; 8]);
    assert!(matches!(
        underlying(
            registry
                .compile_operations(&device, &[write, read])
                .err()
                .unwrap()
        ),
        ContractError::Producer(_)
    ));
    assert!(matches!(
        registry.buffer_view(key, 24..40, 0),
        Err(ContractError::Range(_))
    ));
}

#[test]
fn authored_uploads_are_observed_between_copies_without_waiting() {
    let (device, queue) = device();
    let mut registry = ResourceRegistry::default();
    let scratch = registry
        .import_buffer(
            &buffer(
                &device,
                8,
                wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            ),
            ResourceLifetime::FrameLocal,
            None,
        )
        .unwrap();
    let staging = buffer(
        &device,
        16,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let output = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let mut ops = Vec::new();
    for (index, value) in [17_u8, 93].into_iter().enumerate() {
        let version = u32::try_from(index).unwrap() + 1;
        let view = registry.buffer_view(scratch, 0..8, version).unwrap();
        let mut write = upload(view.clone(), &[value; 8]);
        write.id.occurrence = u32::try_from(index).unwrap();
        ops.push(write);
        ops.push(TypedOperation::new(
            RasterId::new(StageId::new("readback", 0), u32::try_from(index).unwrap()),
            TypedWork::ReadbackBuffer {
                source: view,
                destination: registry
                    .buffer_view(output, index as u64 * 8..index as u64 * 8 + 8, 1)
                    .unwrap(),
            },
        ));
    }
    let compiled = registry.compile_operations(&device, &ops).unwrap();
    assert_eq!(
        compiled.ids().collect::<Vec<_>>(),
        ops.iter().map(|op| op.id).collect::<Vec<_>>()
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, None, |_, _| {})
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = super::operations_padding_tests::read(&device, &staging);
    assert_eq!(bytes, [vec![17; 8], vec![93; 8]].concat());
}

#[test]
fn cube_array_mip_copy_and_pitch_rejection() {
    let (device, queue) = device();
    let mut registry = ResourceRegistry::default();
    let (a, b) = layered_textures(&device, &queue, &mut registry);
    let (copy, mut read, staging) = layered_copies(&device, &mut registry, a, b);
    let compiled = registry
        .compile_operations(&device, &[copy.clone(), read.clone()])
        .unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, None, |_, _| {})
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = super::operations_padding_tests::read(&device, &staging);
    for layer in 0..6 {
        for row in 0..2 {
            assert_eq!(
                &bytes[layer * 512 + row * 256..layer * 512 + row * 256 + 8],
                &[61; 8]
            );
        }
    }
    if let TypedWork::ReadbackTexture { bytes_per_row, .. } = &mut read.work {
        *bytes_per_row = 8;
    }
    assert!(matches!(
        underlying(
            registry
                .compile_operations(&device, &[copy, read])
                .err()
                .unwrap()
        ),
        ContractError::CopyAlignment(_)
    ));
}

fn layered_textures(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    registry: &mut ResourceRegistry,
) -> (AllocationKey, AllocationKey) {
    let make = || {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layered fixture"),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 6,
            },
            mip_level_count: 2,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    };
    let source = make();
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            mip_level: 1,
            ..source.as_image_copy()
        },
        &[61; 96],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 6,
        },
    );
    let a = registry
        .import(
            &source,
            ResourceLifetime::ImportedReadOnly,
            Semantic::Unspecified,
            Some(0),
        )
        .unwrap();
    let b = registry
        .import(
            &make(),
            ResourceLifetime::FrameLocal,
            Semantic::Unspecified,
            None,
        )
        .unwrap();
    (a, b)
}

fn layered_copies(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    a: AllocationKey,
    b: AllocationKey,
) -> (TypedOperation, TypedOperation, wgpu::Buffer) {
    check_cube_binding(device, registry, a);
    let range = TextureRange {
        mips: 1..2,
        layers: 0..6,
        aspect: wgpu::TextureAspect::All,
    };
    let cube = registry
        .view_dimension(a, range.clone(), 0, wgpu::TextureViewDimension::Cube)
        .unwrap();
    assert_eq!(cube.dimension(), wgpu::TextureViewDimension::Cube);
    let target = registry.view(b, range, 1).unwrap();
    let copy = TypedOperation::new(
        id("cube copy"),
        TypedWork::CopyTexture {
            source: TextureCopy::whole(cube),
            destination: TextureCopy::whole(target.clone()),
        },
    );
    let staging = buffer(
        device,
        3072,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let key = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let read = TypedOperation::new(
        id("layer readback"),
        TypedWork::ReadbackTexture {
            source: TextureCopy::whole(target),
            destination: registry.buffer_view(key, 0..3072, 1).unwrap(),
            bytes_per_row: 256,
            rows_per_image: 2,
        },
    );
    (copy, read, staging)
}

#[test]
fn transfer_timing_retains_semantic_operation_identity() {
    let (device, queue) = device();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import_buffer(
            &buffer(&device, 8, wgpu::BufferUsages::COPY_DST),
            ResourceLifetime::FrameLocal,
            None,
        )
        .unwrap();
    let operation = upload(registry.buffer_view(key, 0..8, 1).unwrap(), &[7; 8]);
    let identity = operation.id;
    let compiled = registry.compile_operations(&device, &[operation]).unwrap();
    let mut timer = crate::PassTimer::new(&device, &queue, crate::TimingMode::Optional).unwrap();
    timer.begin_frame(42);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, Some(&mut timer), |_, _| {})
        .unwrap();
    timer.finish_frame(&mut encoder);
    queue.submit([encoder.finish()]);
    if timer.support() == crate::TimingSupport::Supported {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let frames = loop {
            device.poll(wgpu::PollType::Poll).unwrap();
            let frames = timer.take_ready().unwrap();
            if !frames.is_empty() {
                break frames;
            }
            assert!(std::time::Instant::now() < deadline);
        };
        assert_eq!(frames[0].frame_id, 42);
        assert_eq!(
            frames[0].passes[0].pass_id,
            crate::PassId::Operation(identity)
        );
    }
}

fn check_cube_binding(device: &wgpu::Device, registry: &ResourceRegistry, key: AllocationKey) {
    let view = registry
        .view_dimension(
            key,
            TextureRange {
                mips: 0..2,
                layers: 0..6,
                aspect: wgpu::TextureAspect::All,
            },
            0,
            wgpu::TextureViewDimension::Cube,
        )
        .unwrap();
    CheckedBindings::build(
        device,
        registry,
        &[TextureBinding {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            view,
            samples: 1,
            semantic: Semantic::Unspecified,
            kind: TextureBindingKind::Sampled(wgpu::TextureSampleType::Float { filterable: false }),
        }],
    )
    .unwrap();
}
