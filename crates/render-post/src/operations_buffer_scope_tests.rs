use super::operations_tests::{buffer, device, id, underlying};
use crate::contracts::*;

fn operation(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    key: AllocationKey,
    first: BufferBindingKind,
) -> TypedOperation {
    let kinds = [first, BufferBindingKind::Storage(AccessMode::ReadWrite)];
    let entries: Vec<_> = kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| BufferBinding {
            binding: u32::try_from(index).unwrap(),
            visibility: wgpu::ShaderStages::COMPUTE,
            view: registry
                .buffer_view(key, index as u64 * 256..index as u64 * 256 + 16, 1)
                .unwrap(),
            kind,
            dynamic_offset: None,
        })
        .collect();
    // Separate groups ensure validation merges usages across groups as well.
    let groups: Vec<_> = entries
        .iter()
        .map(|entry| {
            CheckedBindings::build_with_buffers(device, registry, &[], std::slice::from_ref(entry))
                .unwrap()
        })
        .collect();
    let pipeline = pipeline(device, &groups, first);
    let mut operation = TypedOperation::new(
        id("buffer scope"),
        TypedWork::Compute {
            pipeline,
            bindings: groups,
            workgroups: [1, 1, 1],
        },
    );
    operation.buffers = entries
        .iter()
        .map(|entry| match entry.kind {
            BufferBindingKind::Uniform => {
                BufferUse::read(entry.view.clone(), BufferUseKind::Uniform)
            }
            BufferBindingKind::Storage(AccessMode::Read) => {
                BufferUse::read(entry.view.clone(), BufferUseKind::Storage)
            }
            BufferBindingKind::Storage(_) => {
                BufferUse::read_write(entry.view.clone(), 0, BufferUseKind::Storage)
            }
        })
        .collect();
    operation
}
#[test]
fn actual_disjoint_bindings_reject_incompatible_whole_buffer_usages() {
    let (device, _) = device();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import_buffer(
            &buffer(
                &device,
                512,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::STORAGE,
            ),
            ResourceLifetime::PersistentMutable,
            Some(1),
        )
        .unwrap();
    for first in [
        BufferBindingKind::Uniform,
        BufferBindingKind::Storage(AccessMode::Read),
    ] {
        let mut operation = operation(&device, &registry, key, first);
        // Read declarations use version 1; writable storage must publish version 2.
        operation.buffers[1].before = Some(1);
        operation.buffers[1].view = registry.buffer_view(key, 256..272, 2).unwrap();
        if let TypedWork::Compute { bindings, .. } = &mut operation.work {
            bindings[1] = CheckedBindings::build_with_buffers(
                &device,
                &registry,
                &[],
                &[BufferBinding {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    view: operation.buffers[1].view.clone(),
                    kind: BufferBindingKind::Storage(AccessMode::ReadWrite),
                    dynamic_offset: None,
                }],
            )
            .unwrap();
        }
        assert!(matches!(
            underlying(
                registry
                    .compile_operations(&device, &[operation])
                    .err()
                    .unwrap()
            ),
            ContractError::BufferUsage(_)
        ));
    }
}
#[test]
fn actual_disjoint_writable_storage_bindings_encode_and_submit() {
    let (device, queue) = device();
    let mut registry = ResourceRegistry::default();
    let key = registry
        .import_buffer(
            &buffer(
                &device,
                512,
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            ),
            ResourceLifetime::PersistentMutable,
            Some(0),
        )
        .unwrap();
    let operation = operation(
        &device,
        &registry,
        key,
        BufferBindingKind::Storage(AccessMode::ReadWrite),
    );
    let staging = buffer(
        &device,
        16,
        wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    );
    let target = registry
        .import_buffer(&staging, ResourceLifetime::FrameLocal, None)
        .unwrap();
    let read = TypedOperation::new(
        id("storage readback"),
        TypedWork::ReadbackBuffer {
            source: registry.buffer_view(key, 256..272, 1).unwrap(),
            destination: registry.buffer_view(target, 0..16, 1).unwrap(),
        },
    );
    let compiled = registry
        .compile_operations(&device, &[operation, read])
        .unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, None, |_, _| {})
        .unwrap();
    queue.submit([encoder.finish()]);
    assert_eq!(
        super::operations_padding_tests::read(&device, &staging),
        [1_u32.to_le_bytes(); 4].concat()
    );
}

fn pipeline(
    device: &wgpu::Device,
    groups: &[CheckedBindings],
    first: BufferBindingKind,
) -> wgpu::ComputePipeline {
    let layouts: Vec<_> = groups.iter().map(|group| Some(group.layout())).collect();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &layouts,
        ..Default::default()
    });
    let first_decl = match first {
        BufferBindingKind::Uniform => "var<uniform>",
        BufferBindingKind::Storage(AccessMode::Read) => "var<storage, read>",
        BufferBindingKind::Storage(_) => "var<storage, read_write>",
    };
    let source = format!(
        "@group(0) @binding(0) {first_decl} a: vec4<u32>; @group(1) @binding(1) var<storage, read_write> b: vec4<u32>; @compute @workgroup_size(1) fn main() {{ b = a + vec4<u32>(1u); }}"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    crate::compute_pipeline(
        device,
        crate::ComputeDescriptor {
            label: None,
            layout: &layout,
            module: &module,
            entry: "main",
            constants: &[],
        },
    )
}
