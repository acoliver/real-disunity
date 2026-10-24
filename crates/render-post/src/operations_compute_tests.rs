use super::operations_tests::{buffer, device, id, underlying, upload};
use crate::contracts::*;

const KERNEL: &str = "
struct Params { value: vec4<u32> }
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> values: array<u32>;
@compute @workgroup_size(1) fn main() { values[params.value.y] = params.value.x; }
";
struct Fixture {
    registry: ResourceRegistry,
    scratch: AllocationKey,
    output: AllocationKey,
    staging: wgpu::Buffer,
    staging_key: AllocationKey,
}
fn fixture(device: &wgpu::Device) -> Fixture {
    let mut registry = ResourceRegistry::default();
    let scratch = registry
        .import_buffer(
            &buffer(
                device,
                512,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ),
            ResourceLifetime::FrameLocal,
            None,
        )
        .unwrap();
    let output = registry
        .import_buffer(
            &buffer(
                device,
                32,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            ),
            ResourceLifetime::PersistentMutable,
            Some(0),
        )
        .unwrap();
    let staging = buffer(
        device,
        8,
        wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    );
    let staging_key = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    Fixture {
        registry,
        scratch,
        output,
        staging,
        staging_key,
    }
}
fn bindings(
    device: &wgpu::Device,
    f: &Fixture,
    version: u32,
    offset: u32,
) -> Result<CheckedBindings, ContractError> {
    CheckedBindings::build_with_buffers(
        device,
        &f.registry,
        &[],
        &[
            BufferBinding {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                view: f.registry.buffer_view(f.scratch, 0..16, version).unwrap(),
                kind: BufferBindingKind::Uniform,
                dynamic_offset: Some(offset),
            },
            BufferBinding {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                view: f.registry.buffer_view(f.output, 0..8, version).unwrap(),
                kind: BufferBindingKind::Storage(AccessMode::ReadWrite),
                dynamic_offset: None,
            },
        ],
    )
}
fn compute(device: &wgpu::Device, f: &Fixture, version: u32) -> TypedOperation {
    let group = bindings(device, f, version, 256).unwrap();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(group.layout())],
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("scratch fixture"),
        source: wgpu::ShaderSource::Wgsl(KERNEL.into()),
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
        crate::contracts::RasterId::new(crate::StageId::new("scratch dispatch", 0), version),
        TypedWork::Compute {
            pipeline,
            bindings: vec![group],
            workgroups: [1, 1, 1],
        },
    );
    operation.buffers = vec![
        BufferUse::read(
            f.registry
                .buffer_view(f.scratch, 256..272, version)
                .unwrap(),
            BufferUseKind::Uniform,
        ),
        BufferUse::read_write(
            f.registry.buffer_view(f.output, 0..8, version).unwrap(),
            version - 1,
            BufferUseKind::Storage,
        ),
    ];
    operation
}
#[test]
fn dynamic_offsets_and_interleaved_uniform_updates_reach_correct_dispatch() {
    let (device, queue) = device();
    let f = fixture(&device);
    assert!(matches!(
        bindings(&device, &f, 1, 1),
        Err(ContractError::DynamicOffset(_))
    ));
    assert!(matches!(
        bindings(&device, &f, 1, 512),
        Err(ContractError::DynamicOffset(_))
    ));
    let mut operations = Vec::new();
    for (index, value) in [17_u32, 93].into_iter().enumerate() {
        let version = u32::try_from(index).unwrap() + 1;
        let bytes: Vec<_> = [value, version - 1, 0, 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let mut write = upload(
            f.registry
                .buffer_view(f.scratch, 256..272, version)
                .unwrap(),
            &bytes,
        );
        write.id.occurrence = version;
        operations.extend([write, compute(&device, &f, version)]);
    }
    operations.push(TypedOperation::new(
        id("readback"),
        TypedWork::ReadbackBuffer {
            source: f.registry.buffer_view(f.output, 0..8, 2).unwrap(),
            destination: f.registry.buffer_view(f.staging_key, 0..8, 1).unwrap(),
        },
    ));
    let compiled = f.registry.compile_operations(&device, &operations).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&f.registry, &mut encoder, None, |_, _| {})
        .unwrap();
    queue.submit([encoder.finish()]);
    let bytes = super::operations_padding_tests::read(&device, &f.staging);
    assert_eq!(bytes, [17_u32.to_le_bytes(), 93_u32.to_le_bytes()].concat());
}
#[test]
fn compute_rejects_missing_declarations_producers_hazards_and_dispatch() {
    let (device, _) = device();
    let f = fixture(&device);
    let operation = compute(&device, &f, 1);
    assert!(matches!(
        underlying(
            f.registry
                .compile_operations(&device, std::slice::from_ref(&operation))
                .err()
                .unwrap()
        ),
        ContractError::Producer(_)
    ));
    let write = upload(
        f.registry.buffer_view(f.scratch, 256..272, 1).unwrap(),
        &[0; 16],
    );
    let mut missing = operation.clone();
    missing.buffers.pop();
    assert!(matches!(
        underlying(
            f.registry
                .compile_operations(&device, &[write.clone(), missing])
                .err()
                .unwrap()
        ),
        ContractError::Binding(_)
    ));
    let mut hazard = operation.clone();
    hazard.buffers.push(hazard.buffers[1].clone());
    assert!(matches!(
        underlying(
            f.registry
                .compile_operations(&device, &[write.clone(), hazard])
                .err()
                .unwrap()
        ),
        ContractError::Hazard(_)
    ));
    let mut dispatch = operation;
    if let TypedWork::Compute { workgroups, .. } = &mut dispatch.work {
        *workgroups = [0, 1, 1];
    }
    assert!(matches!(
        underlying(
            f.registry
                .compile_operations(&device, &[write, dispatch])
                .err()
                .unwrap()
        ),
        ContractError::Dispatch(_)
    ));
}

#[test]
fn disjoint_buffer_ranges_are_allowed_but_overlaps_fail() {
    let (device, _) = device();
    let f = fixture(&device);
    let write = upload(
        f.registry.buffer_view(f.scratch, 256..272, 1).unwrap(),
        &[0; 16],
    );
    let mut dispatch = compute(&device, &f, 1);
    dispatch.buffers.push(BufferUse::read(
        f.registry.buffer_view(f.output, 8..16, 0).unwrap(),
        BufferUseKind::Storage,
    ));
    assert!(
        f.registry
            .compile_operations(&device, &[write.clone(), dispatch.clone()])
            .is_ok()
    );
    dispatch.buffers.last_mut().unwrap().view = f.registry.buffer_view(f.output, 4..12, 0).unwrap();
    assert!(matches!(
        underlying(
            f.registry
                .compile_operations(&device, &[write, dispatch])
                .err()
                .unwrap()
        ),
        ContractError::Hazard(_)
    ));
}
