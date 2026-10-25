use super::geometry::{fixture, mesh};
use crate::bridge_support::*;
use worldrunner_render_post::contracts::*;

#[test]
fn nonindexed_vertex_stream_checks_ranges_alignment_and_instances() {
    let (device, authority) = gpu();
    let mut registry = ResourceRegistry::default();
    let mut fixture = fixture(&device, &mut registry);
    fixture.geometry.index = None;
    fixture.geometry.base_vertex = 0;
    let output = texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
    let target = view(&mut registry, &output, 0, 1);
    let raster = pass("nonindexed vertex stream", target);
    let packet = mesh(
        &device,
        fixture.geometry.clone(),
        fixture.bindings.clone(),
        false,
    )
    .unwrap();
    let mut op = TypedOperation::new(
        raster.id,
        TypedWork::Draws {
            pass: raster,
            draws: vec![packet],
        },
    );
    op.buffers = fixture.declarations;
    execute(&device, &authority, &registry, &[op]);
    assert_eq!(read(&device, &authority, &output), [255, 0, 0, 255]);
    let mut invalid = fixture.geometry.clone();
    invalid.instances = 1..4;
    assert!(matches!(
        mesh(&device, invalid, fixture.bindings.clone(), false),
        Err(ContractError::Range(_))
    ));
    let mut invalid = fixture.geometry.clone();
    invalid.elements = 1..5;
    assert!(matches!(
        mesh(&device, invalid, fixture.bindings.clone(), false),
        Err(ContractError::Range(_))
    ));
    let mut invalid = fixture.geometry;
    invalid.vertex_buffers[0] = registry
        .buffer_view(invalid.vertex_buffers[0].allocation(), 2..34, 0)
        .unwrap();
    assert!(matches!(
        mesh(&device, invalid, fixture.bindings, false),
        Err(ContractError::Binding(_))
    ));
}
#[test]
fn actual_vertex_stream_participates_in_whole_buffer_storage_hazards() {
    let (device, _) = gpu();
    let mut registry = ResourceRegistry::default();
    let fixture = fixture(&device, &mut registry);
    let storage = registry
        .buffer_view(fixture.geometry.vertex_buffers[0].allocation(), 0..8, 1)
        .unwrap();
    let bindings = CheckedBindings::build_with_buffers(
        &device,
        &registry,
        &[],
        &[
            BufferBinding {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                view: fixture.uniform,
                kind: BufferBindingKind::Uniform,
                dynamic_offset: Some(device.limits().min_uniform_buffer_offset_alignment),
            },
            BufferBinding {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                view: storage.clone(),
                kind: BufferBindingKind::Storage(AccessMode::Write),
                dynamic_offset: None,
            },
        ],
    )
    .unwrap();
    let packet = mesh(&device, fixture.geometry, bindings, false).unwrap();
    let output = texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
    let target = view(&mut registry, &output, 0, 1);
    let raster = pass("vertex storage hazard", target);
    let mut op = TypedOperation::new(
        raster.id,
        TypedWork::Draws {
            pass: raster,
            draws: vec![packet],
        },
    );
    op.buffers = fixture.declarations;
    op.buffers
        .push(BufferUse::write(storage, BufferUseKind::Storage));
    assert!(
        matches!(registry.compile_operations(&device, &[op]), Err(ContractError::Pass { source, .. }) if matches!(*source, ContractError::BufferUsage(_)))
    );
}
