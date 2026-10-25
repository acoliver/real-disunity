use crate::bridge_support::*;
use wgpu::util::DeviceExt;
use worldrunner_render_post::{AttachmentLoad, CopyOutput, contracts::*};

fn buffer(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
    bytes: &[u8],
    usage: wgpu::BufferUsages,
    range: std::ops::Range<u64>,
) -> BufferView {
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytes,
        usage,
    });
    let key = registry
        .import_buffer(&buffer, ResourceLifetime::PersistentMutable, Some(0))
        .unwrap();
    registry.buffer_view(key, range, 0).unwrap()
}
pub(super) fn mesh(
    device: &wgpu::Device,
    geometry: DrawGeometry,
    bindings: CheckedBindings,
    depth_only: bool,
) -> Result<CheckedDraw, ContractError> {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl("@group(0) @binding(0) var<uniform> color: vec4<f32>; @vertex fn vs(@location(0) p: vec2<f32>, @location(1) offset: vec2<f32>) -> @builtin(position) vec4<f32> { return vec4<f32>(p + offset, 0.25, 1.); } @fragment fn fs() -> @location(0) vec4<f32> { return color; }".into()) });
    let attributes = [wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let instance_attributes = [wgpu::VertexAttribute {
        shader_location: 1,
        ..attributes[0]
    }];
    let layouts = [
        Some(wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &instance_attributes,
        }),
    ];
    let mut descriptor = descriptor(&module, 1);
    descriptor.vertex.buffers = &layouts;
    if depth_only {
        descriptor.fragment = None;
        descriptor.depth_stencil = Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
    }
    CheckedDraw::with_geometry(device, &descriptor, vec![bindings], geometry)
}
pub(super) struct Fixture {
    pub(super) geometry: DrawGeometry,
    pub(super) bindings: CheckedBindings,
    pub(super) declarations: Vec<BufferUse>,
    pub(super) uniform: BufferView,
}
pub(super) fn fixture(device: &wgpu::Device, registry: &mut ResourceRegistry) -> Fixture {
    let vertices: [f32; 10] = [99., 99., 99., 99., -1., -1., 3., -1., -1., 3.];
    let vertices = buffer(
        device,
        registry,
        &float_bytes(&vertices),
        wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
        8..40,
    );
    let instances = buffer(
        device,
        registry,
        &float_bytes(&[99_f32, 99., 99., 99., 0., 0., 0., 0.]),
        wgpu::BufferUsages::VERTEX,
        8..32,
    );
    let indices = buffer(
        device,
        registry,
        &[99_u16, 99, 99, 0, 1, 2]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
        wgpu::BufferUsages::INDEX,
        4..12,
    );
    let (uniform, binding) = uniform_binding(device, registry);
    let declarations = vec![
        BufferUse::read(vertices.clone(), BufferUseKind::Vertex),
        BufferUse::read(instances.clone(), BufferUseKind::Vertex),
        BufferUse::read(indices.clone(), BufferUseKind::Index),
        BufferUse::read(binding.effective().unwrap(), BufferUseKind::Uniform),
    ];
    let bindings = CheckedBindings::build_with_buffers(device, registry, &[], &[binding]).unwrap();
    Fixture {
        geometry: DrawGeometry {
            vertex_buffers: vec![vertices, instances],
            index: Some(IndexBinding {
                view: indices,
                format: wgpu::IndexFormat::Uint16,
            }),
            elements: 1..4,
            base_vertex: 1,
            instances: 1..3,
        },
        bindings,
        declarations,
        uniform,
    }
}
#[test]
fn indexed_instanced_mesh_dynamic_offset_and_stock_share_one_pass() {
    let (device, authority) = gpu();
    let mut registry = ResourceRegistry::default();
    let fixture = fixture(&device, &mut registry);
    let input = texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
    let source = view(&mut registry, &input, 0, 1);
    let init = pass("geometry stock source", source.clone());
    let output = texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
    let target = view(&mut registry, &output, 0, 1);
    let mut mixed = pass("geometry mixed providers", target);
    mixed
        .uses
        .push(ResourceUse::read(source.clone(), UseKind::Sampled));
    let draws = vec![
        CopyOutput::checked_draw(&device, &registry, source, 1).unwrap(),
        mesh(
            &device,
            fixture.geometry.clone(),
            fixture.bindings.clone(),
            false,
        )
        .unwrap(),
    ];
    let mut operation = TypedOperation::new(mixed.id, TypedWork::Draws { pass: mixed, draws });
    assert!(
        registry
            .compile_operations(&device, std::slice::from_ref(&operation))
            .is_err()
    );
    operation.buffers = fixture.declarations.clone();
    let initialize = TypedOperation::new(
        init.id,
        TypedWork::Draws {
            pass: init,
            draws: vec![],
        },
    );
    execute(
        &device,
        &authority,
        &registry,
        &[initialize.clone(), operation.clone()],
    );
    assert_eq!(read(&device, &authority, &output), [255, 0, 0, 255]);
    geometry_negatives(
        &device,
        &authority,
        &mut registry,
        fixture,
        initialize,
        operation,
    );
}
fn geometry_negatives(
    device: &wgpu::Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    registry: &mut ResourceRegistry,
    fixture: Fixture,
    initialize: TypedOperation,
    mut operation: TypedOperation,
) {
    operation.buffers[0].view = registry
        .buffer_view(operation.buffers[0].view.allocation(), 8..40, 1)
        .unwrap();
    assert!(
        registry
            .compile_operations(device, &[initialize.clone(), operation.clone()])
            .is_err()
    );
    operation.buffers[0].view = fixture.geometry.vertex_buffers[0].clone();
    let compiled = registry
        .compile_operations(device, &[initialize, operation])
        .unwrap();
    let replacement = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 40,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: false,
    });
    registry
        .replace_buffer(
            fixture.geometry.vertex_buffers[0].allocation(),
            &replacement,
        )
        .unwrap();
    assert!(
        authority
            .prepare(device, |encoder, _| compiled
                .encode_checked(registry, encoder, None))
            .is_err()
    );
    let mut invalid = fixture.geometry;
    invalid.elements = 1..5;
    assert!(matches!(
        mesh(device, invalid, fixture.bindings, false),
        Err(ContractError::Range(_))
    ));
    assert_eq!(fixture.uniform.range(), 0..16);
}
#[test]
fn depth_only_mesh_selects_array_layer() {
    let (device, authority) = gpu();
    let mut registry = ResourceRegistry::default();
    let fixture = fixture(&device, &mut registry);
    let depth = texture(&device, wgpu::TextureFormat::Depth32Float, 1, 2);
    let selected = view(&mut registry, &depth, 1, 1);
    let pass = RasterPass {
        id: id("selected depth layer"),
        colors: vec![],
        depth: Some(DepthAttachment {
            view: selected.clone(),
            before: None,
            load: AttachmentLoad::ClearDepth(1.),
            store: wgpu::StoreOp::Store,
            read_only: false,
        }),
        uses: vec![],
        bindings: vec![],
    };
    let mut draw = TypedOperation::new(
        pass.id,
        TypedWork::Draws {
            pass,
            draws: vec![mesh(&device, fixture.geometry, fixture.bindings, true).unwrap()],
        },
    );
    draw.buffers = fixture.declarations;
    let output = texture(&device, wgpu::TextureFormat::Rgba8Unorm, 1, 1);
    let target = view(&mut registry, &output, 0, 1);
    let group = CheckedBindings::build(
        &device,
        &registry,
        &[TextureBinding {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            view: selected.clone(),
            samples: 1,
            semantic: Semantic::DeviceDepth,
            kind: TextureBindingKind::Sampled(wgpu::TextureSampleType::Depth),
        }],
    )
    .unwrap();
    let shader = format!(
        "{VERTEX} @group(0) @binding(0) var d: texture_depth_2d; @fragment fn fs() -> @location(0) vec4<f32> {{ return vec4<f32>(textureLoad(d, vec2<i32>(0), 0), 0., 0., 1.); }}"
    );
    let packet = crate::bridge_support::draw(&device, &shader, vec![group], 1);
    let snapshot = pass_for_depth(target, selected);
    let snapshot = TypedOperation::new(
        snapshot.id,
        TypedWork::Draws {
            pass: snapshot,
            draws: vec![packet],
        },
    );
    execute(&device, &authority, &registry, &[draw, snapshot]);
    assert_eq!(read(&device, &authority, &output), [64, 0, 0, 255]);
}
fn pass_for_depth(target: CheckedView, source: CheckedView) -> RasterPass {
    let mut pass = pass("depth layer snapshot", target);
    pass.uses.push(ResourceUse::read(source, UseKind::Sampled));
    pass
}

pub(super) fn float_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn uniform_binding(
    device: &wgpu::Device,
    registry: &mut ResourceRegistry,
) -> (BufferView, BufferBinding) {
    let alignment = device.limits().min_uniform_buffer_offset_alignment;
    let mut bytes = vec![0; alignment as usize + 16];
    bytes[alignment as usize..].copy_from_slice(&float_bytes(&[1_f32, 0., 0., 1.]));
    let uniform = buffer(
        device,
        registry,
        &bytes,
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        0..16,
    );
    let binding = BufferBinding {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        view: uniform.clone(),
        kind: BufferBindingKind::Uniform,
        dynamic_offset: Some(alignment),
    };
    (uniform, binding)
}
