fn storage_array(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 2,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

#[test]
fn storage_views_and_declared_access_are_checked() {
    let (device, _queue) = device();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let array = storage_array(&device);
    let mut registry = ResourceRegistry::default();
    let key = import(&mut registry, &array);
    let range = TextureRange {
        mips: 0..1,
        layers: 0..2,
        aspect: wgpu::TextureAspect::All,
    };
    let selected = registry.view(key, range.clone(), 0).unwrap();
    let entry = binding(
        selected.clone(),
        TextureBindingKind::Storage(wgpu::StorageTextureAccess::ReadOnly),
    );
    let checked = CheckedBindings::build(&device, &registry, std::slice::from_ref(&entry)).unwrap();
    let multiple_mips = registry
        .view(
            key,
            TextureRange {
                mips: 0..2,
                ..range
            },
            0,
        )
        .unwrap();
    assert!(matches!(
        CheckedBindings::build(
            &device,
            &registry,
            &[TextureBinding {
                view: multiple_mips,
                ..entry
            }]
        ),
        Err(ContractError::StorageAccess { .. })
    ));
    let mut consumer = reader(&device, &mut registry, "storage declarations");
    consumer
        .uses
        .push(ResourceUse::write(selected.clone(), UseKind::Storage));
    assert_eq!(
        checked.validate(&registry, &consumer),
        Err(ContractError::Binding(key))
    );
    consumer.uses[0] = ResourceUse::read(selected, UseKind::Storage);
    consumer.bindings.push(checked);
    registry.compile(&device, &[consumer]).unwrap();
    let entry = binding(
        view(&registry, key, 1),
        TextureBindingKind::Storage(wgpu::StorageTextureAccess::ReadWrite),
    );
    CheckedBindings::build(&device, &registry, &[entry]).unwrap();
    no_gpu_errors(&device, scope);
}

#[test]
fn writable_storage_cannot_bind_a_read_only_import() {
    let (device, _queue) = device();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut registry = ResourceRegistry::default();
    let read_only = registry
        .import(
            &typed_texture(&device, wgpu::TextureFormat::R32Float, true),
            ResourceLifetime::ImportedReadOnly,
            Semantic::Unspecified,
            Some(0),
        )
        .unwrap();
    let entry = binding(
        view(&registry, read_only, 0),
        TextureBindingKind::Storage(wgpu::StorageTextureAccess::WriteOnly),
    );
    assert!(
        matches!(CheckedBindings::build(&device, &registry, &[entry]), Err(ContractError::ReadOnly(key)) if key == read_only)
    );
    no_gpu_errors(&device, scope);
}

use super::*;

fn typed_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    storage: bool,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capability fixture"),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | if storage {
                wgpu::TextureUsages::STORAGE_BINDING
            } else {
                wgpu::TextureUsages::empty()
            },
        view_formats: &[],
    })
}

fn import(registry: &mut ResourceRegistry, texture: &wgpu::Texture) -> AllocationKey {
    registry
        .import(
            texture,
            ResourceLifetime::FrameLocal,
            Semantic::Unspecified,
            Some(0),
        )
        .unwrap()
}

fn binding(view: CheckedView, kind: TextureBindingKind) -> TextureBinding {
    TextureBinding {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        view,
        samples: 1,
        semantic: Semantic::Unspecified,
        kind,
    }
}

fn no_gpu_errors(device: &wgpu::Device, scope: wgpu::ErrorScopeGuard) {
    device.poll(wgpu::PollType::Poll).unwrap();
    let error = pollster::block_on(scope.pop());
    assert!(
        error.is_none(),
        "unexpected GPU validation error: {error:?}"
    );
}

#[test]
fn sampled_contracts_fail_before_gpu_layout_creation() {
    let (device, _queue) = device();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut registry = ResourceRegistry::default();
    let key = import(&mut registry, &texture(&device, 4));
    let mut entry = binding(
        view(&registry, key, 0),
        TextureBindingKind::Sampled(wgpu::TextureSampleType::Float { filterable: true }),
    );
    entry.samples = 4;
    assert!(
        matches!(CheckedBindings::build(&device, &registry, &[entry.clone()]),
        Err(ContractError::SampleType(found)) if found == key)
    );
    entry.kind = TextureBindingKind::Sampled(wgpu::TextureSampleType::Float { filterable: false });
    CheckedBindings::build(&device, &registry, &[entry.clone()]).unwrap();
    entry.kind = TextureBindingKind::Sampled(wgpu::TextureSampleType::Uint);
    assert!(matches!(
        CheckedBindings::build(&device, &registry, &[entry]),
        Err(ContractError::SampleType(_))
    ));
    no_gpu_errors(&device, scope);
}

#[test]
fn storage_access_and_visibility_fail_before_gpu_layout_creation() {
    let (device, _queue) = device();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut registry = ResourceRegistry::default();
    let key = import(
        &mut registry,
        &typed_texture(&device, wgpu::TextureFormat::Rgba8Unorm, true),
    );
    for access in [
        wgpu::StorageTextureAccess::ReadWrite,
        wgpu::StorageTextureAccess::Atomic,
    ] {
        let entry = binding(view(&registry, key, 0), TextureBindingKind::Storage(access));
        assert!(
            matches!(CheckedBindings::build(&device, &registry, &[entry]),
            Err(ContractError::StorageAccess { allocation, access: found, .. }) if allocation == key && found == access)
        );
    }
    for access in [
        wgpu::StorageTextureAccess::ReadOnly,
        wgpu::StorageTextureAccess::WriteOnly,
    ] {
        let entry = binding(view(&registry, key, 0), TextureBindingKind::Storage(access));
        CheckedBindings::build(&device, &registry, &[entry]).unwrap();
    }
    let mut entry = binding(
        view(&registry, key, 0),
        TextureBindingKind::Storage(wgpu::StorageTextureAccess::WriteOnly),
    );
    entry.visibility = wgpu::ShaderStages::VERTEX;
    assert!(matches!(
        CheckedBindings::build(&device, &registry, &[entry]),
        Err(ContractError::StorageAccess { .. })
    ));
    no_gpu_errors(&device, scope);
}

#[test]
fn binding_indices_visibility_and_stage_counts_are_checked() {
    let (device, _queue) = device();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut registry = ResourceRegistry::default();
    let key = import(&mut registry, &texture(&device, 1));
    let mut entry = binding(
        view(&registry, key, 0),
        TextureBindingKind::Sampled(wgpu::TextureSampleType::Float { filterable: false }),
    );
    entry.binding = device.limits().max_bindings_per_bind_group;
    assert!(matches!(
        CheckedBindings::build(&device, &registry, &[entry.clone()]),
        Err(ContractError::BindingLayout(_))
    ));
    entry.binding = 0;
    entry.visibility = wgpu::ShaderStages::from_bits_retain(1 << 31);
    assert!(matches!(
        CheckedBindings::build(&device, &registry, &[entry.clone()]),
        Err(ContractError::BindingLayout(_))
    ));
    entry.visibility = wgpu::ShaderStages::FRAGMENT;
    let entries: Vec<_> = (0..=device.limits().max_sampled_textures_per_shader_stage)
        .map(|index| TextureBinding {
            binding: index,
            ..entry.clone()
        })
        .collect();
    assert!(matches!(
        CheckedBindings::build(&device, &registry, &entries),
        Err(ContractError::BindingLayout(_))
    ));
    no_gpu_errors(&device, scope);
}

fn mrt(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    formats: &[wgpu::TextureFormat],
) -> RasterPass {
    let views: Vec<_> = formats
        .iter()
        .map(|format| {
            let key = import(registry, &typed_texture(device, *format, false));
            view(registry, key, 1)
        })
        .collect();
    pass("byte budget", &views)
}

#[test]
fn mrt_target_cost_includes_component_alignment_and_holes() {
    use wgpu::TextureFormat::{R8Unorm, Rgba8Unorm, Rgba16Float, Rgba32Float};
    let (device, _queue) = device();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    assert_eq!(device.limits().max_color_attachment_bytes_per_sample, 32);
    let mut registry = ResourceRegistry::default();
    let over = mrt(&device, &mut registry, &[Rgba32Float; 3]);
    assert!(
        matches!(registry.compile(&device, &[over]), Err(ContractError::Pass { source, .. })
        if *source == ContractError::ColorAttachmentBytes { total: 48, limit: 32 })
    );
    let mut exact = mrt(
        &device,
        &mut registry,
        &[Rgba32Float, Rgba8Unorm, Rgba16Float],
    );
    exact.colors.insert(1, None);
    let compiled = registry.compile(&device, &[exact]).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled.encode(&registry, &mut encoder, |_, _| {}).unwrap();
    let aligned_over = mrt(
        &device,
        &mut registry,
        &[Rgba32Float, R8Unorm, Rgba16Float, Rgba8Unorm],
    );
    assert!(
        matches!(registry.compile(&device, &[aligned_over]), Err(ContractError::Pass { source, .. })
        if *source == ContractError::ColorAttachmentBytes { total: 34, limit: 32 })
    );
    no_gpu_errors(&device, scope);
}

#[test]
fn read_only_replacement_is_rejected_without_mutating_identity_or_validity() {
    let (device, _queue) = device();
    let mut registry = ResourceRegistry::default();
    let original = texture(&device, 1);
    let key = registry
        .import(
            &original,
            ResourceLifetime::ImportedReadOnly,
            Semantic::Unspecified,
            Some(0),
        )
        .unwrap();
    let old = view(&registry, key, 0);
    let checked = CheckedBindings::build(
        &device,
        &registry,
        &[binding(
            old.clone(),
            TextureBindingKind::Sampled(wgpu::TextureSampleType::Float { filterable: false }),
        )],
    )
    .unwrap();
    let replacement = texture(&device, 1);
    assert_eq!(
        registry.replace(key, &replacement),
        Err(ContractError::ReadOnlyReplacement(key))
    );
    registry.check_view(&old).unwrap();
    let mut consumer = reader(&device, &mut registry, "read only");
    consumer.uses.push(ResourceUse::read(old, UseKind::Sampled));
    consumer.bindings.push(checked);
    registry.compile(&device, &[consumer.clone()]).unwrap();
    let next = registry
        .import(
            &replacement,
            ResourceLifetime::ImportedReadOnly,
            Semantic::Unspecified,
            Some(7),
        )
        .unwrap();
    consumer.bindings.clear();
    consumer.uses[0] = ResourceUse::read(view(&registry, next, 7), UseKind::Sampled);
    registry.compile(&device, &[consumer]).unwrap();
    assert!(
        registry
            .import(
                &original,
                ResourceLifetime::ImportedReadOnly,
                Semantic::Unspecified,
                Some(1)
            )
            .is_err()
    );
}

#[test]
fn mutable_replacement_still_invalidates_checked_bindings() {
    let (device, _queue) = device();
    let mut registry = ResourceRegistry::default();
    let key = import(&mut registry, &texture(&device, 1));
    let old = view(&registry, key, 0);
    let checked = CheckedBindings::build(
        &device,
        &registry,
        &[binding(
            old.clone(),
            TextureBindingKind::Sampled(wgpu::TextureSampleType::Float { filterable: false }),
        )],
    )
    .unwrap();
    let mut consumer = reader(&device, &mut registry, "mutable read");
    consumer.uses.push(ResourceUse::read(old, UseKind::Sampled));
    checked.validate(&registry, &consumer).unwrap();
    let next = registry.replace(key, &texture(&device, 1)).unwrap();
    assert_eq!(next.generation(), key.generation() + 1);
    assert_eq!(
        checked.validate(&registry, &consumer),
        Err(ContractError::StaleAllocation(key))
    );
    consumer.uses[0].view = view(&registry, next, 0);
    assert!(
        matches!(registry.compile(&device, &[consumer]), Err(ContractError::Pass { source, .. })
        if *source == ContractError::Producer(next))
    );
}

fn depth_pass(view: CheckedView, read_only: bool) -> RasterPass {
    RasterPass {
        depth: Some(DepthAttachment {
            view,
            before: read_only.then_some(0),
            load: if read_only {
                AttachmentLoad::Load
            } else {
                AttachmentLoad::ClearDepth(1.0)
            },
            store: wgpu::StoreOp::Store,
            read_only,
        }),
        ..pass("depth access", &[])
    }
}

#[test]
fn writable_depth_feedback_and_storage_writes_remain_hazards() {
    let (device, _queue) = device();
    let mut registry = ResourceRegistry::default();
    let key = import(
        &mut registry,
        &typed_texture(&device, wgpu::TextureFormat::Depth32Float, false),
    );
    let mut depth = depth_pass(view(&registry, key, 1), false);
    depth
        .uses
        .push(ResourceUse::read(view(&registry, key, 0), UseKind::Sampled));
    assert_hazard(&device, &registry, depth, key);
    let key = import(
        &mut registry,
        &typed_texture(&device, wgpu::TextureFormat::R32Float, true),
    );
    let mut storage = reader(&device, &mut registry, "storage hazard");
    storage.uses = vec![
        ResourceUse::read(view(&registry, key, 0), UseKind::Sampled),
        ResourceUse::write(view(&registry, key, 1), UseKind::Storage),
    ];
    assert_hazard(&device, &registry, storage.clone(), key);
    storage.uses[0] = ResourceUse::write(view(&registry, key, 2), UseKind::Storage);
    assert_hazard(&device, &registry, storage, key);
    let mut color = pass("color feedback", &[view(&registry, key, 1)]);
    color
        .uses
        .push(ResourceUse::read(view(&registry, key, 0), UseKind::Sampled));
    assert_hazard(&device, &registry, color, key);
}

fn assert_hazard(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    pass: RasterPass,
    key: AllocationKey,
) {
    assert!(
        matches!(registry.compile(device, &[pass]), Err(ContractError::Pass { source, .. })
        if *source == ContractError::Hazard(key))
    );
}

fn sampled_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    storage: bool,
) -> wgpu::RenderPipeline {
    let declarations = if storage {
        "@group(0) @binding(0) var input: texture_2d<f32>;\n@group(0) @binding(1) var other: texture_storage_2d<r32float, read>;"
    } else {
        "@group(0) @binding(0) var input: texture_depth_2d;"
    };
    let value = if storage {
        "0.5 * (textureLoad(input, vec2<i32>(0), 0).x + textureLoad(other, vec2<i32>(0)).x)"
    } else {
        "textureLoad(input, vec2<i32>(0), 0)"
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(
            format!(
                "{declarations}\n
            @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
                let p = array(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
                return vec4(p[i], 0.0, 1.0);
            }}
            @fragment fn fs() -> @location(0) vec4<f32> {{ return vec4(0.0, {value}, 0.0, 1.0); }}"
            )
            .into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[Some(layout)],
        ..Default::default()
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
        depth_stencil: (!storage).then_some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
    })
}

#[test]
fn compatible_read_only_roles_execute_checked_sampled_draws() {
    let (device, queue) = device();
    for storage in [false, true] {
        execute_read_only_draw(&device, &queue, storage);
    }
}

fn read_only_source(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    storage: bool,
) -> (AllocationKey, CheckedBindings) {
    let format = if storage {
        wgpu::TextureFormat::R32Float
    } else {
        wgpu::TextureFormat::Depth32Float
    };
    let key = registry
        .import(
            &typed_texture(device, format, storage),
            ResourceLifetime::FrameLocal,
            Semantic::Unspecified,
            None,
        )
        .unwrap();
    let source = view(registry, key, 0);
    let mut entries = vec![binding(
        source.clone(),
        TextureBindingKind::Sampled(if storage {
            wgpu::TextureSampleType::Float { filterable: false }
        } else {
            wgpu::TextureSampleType::Depth
        }),
    )];
    if storage {
        entries.push(TextureBinding {
            binding: 1,
            ..binding(
                source.clone(),
                TextureBindingKind::Storage(wgpu::StorageTextureAccess::ReadOnly),
            )
        });
    }
    let checked = CheckedBindings::build(device, registry, &entries).unwrap();
    (key, checked)
}

fn execute_read_only_draw(device: &wgpu::Device, queue: &wgpu::Queue, storage: bool) {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut registry = ResourceRegistry::default();
    let (key, checked) = read_only_source(device, &mut registry, storage);
    let source = view(&registry, key, 0);
    let pipeline = sampled_pipeline(device, checked.layout(), storage);
    let output = texture(device, 1);
    let output_key = import(&mut registry, &output);
    let mut consumer = pass("compatible reads", &[view(&registry, output_key, 1)]);
    consumer
        .uses
        .push(ResourceUse::read(source.clone(), UseKind::Sampled));
    let producer = if storage {
        consumer
            .uses
            .push(ResourceUse::read(source, UseKind::Storage));
        pass("initialize storage", &[view(&registry, key, 0)])
    } else {
        consumer.depth = depth_pass(source, true).depth;
        depth_pass(view(&registry, key, 0), false)
    };
    consumer.bindings.push(checked);
    let compiled = registry.compile(device, &[producer, consumer]).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    compiled
        .encode(&registry, &mut encoder, |id, render| {
            if id.stage.name == "compatible reads" {
                render.set_pipeline(&pipeline);
                render.draw(0..3, 0..1);
            }
        })
        .unwrap();
    let submission = queue.submit([encoder.finish()]);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    let mut bytes = crate::Readback::new(
        device,
        crate::FrameConfig::new([4, 4], wgpu::TextureFormat::Rgba8Unorm, 1),
        4096,
    )
    .unwrap();
    assert!(
        bytes
            .read(device, queue, &output)
            .unwrap()
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 255, 0, 255])
    );
    no_gpu_errors(device, scope);
}
