use crate::bridge_support::*;
use worldrunner_render_post::{CopyOutput, contracts::*};

fn no_errors(device: &wgpu::Device, scope: wgpu::ErrorScopeGuard) {
    device.poll(wgpu::PollType::Poll).unwrap();
    assert!(pollster::block_on(scope.pop()).is_none());
}
#[test]
fn aggregate_dynamic_and_stage_limits_fail_before_pipeline_creation() {
    let (device, _) = gpu();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut registry = ResourceRegistry::default();
    let group = dynamic_group(&device, &mut registry);
    let shader =
        format!("{VERTEX} @fragment fn fs() -> @location(0) vec4<f32> {{ return vec4<f32>(1.); }}");
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });
    assert!(matches!(
        CheckedDraw::new(
            &device,
            &descriptor(&module, 1),
            vec![group.clone(), group],
            0..3,
            0..1
        ),
        Err(ContractError::Pipeline)
    ));
    let samplers: Vec<_> = (0..=device.limits().max_samplers_per_shader_stage / 2)
        .map(|binding| SamplerBinding {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            sampler: CheckedSampler::new(&device, &wgpu::SamplerDescriptor::default()),
        })
        .collect();
    let group =
        CheckedBindings::build_with_resources(&device, &registry, &[], &[], &samplers).unwrap();
    assert!(matches!(
        CheckedDraw::new(
            &device,
            &descriptor(&module, 1),
            vec![group.clone(), group],
            0..3,
            0..1
        ),
        Err(ContractError::Pipeline)
    ));
    no_errors(&device, scope);
}
#[test]
fn checked_copy_rejects_array_cube_integer_and_invalid_samples_without_gpu_errors() {
    let (device, _) = gpu();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut registry = ResourceRegistry::default();
    for (format, layers, dimension, samples) in invalid_copy_cases() {
        let input = texture(&device, format, 1, layers);
        let key = registry
            .import(
                &input,
                ResourceLifetime::ImportedReadOnly,
                Semantic::LinearColor,
                Some(0),
            )
            .unwrap();
        let source = registry
            .view_dimension(
                key,
                TextureRange {
                    mips: 0..1,
                    layers: 0..layers,
                    aspect: wgpu::TextureAspect::All,
                },
                0,
                dimension,
            )
            .unwrap();
        assert!(
            matches!(CopyOutput::checked_draw(&device, &registry, source, samples), Err(ContractError::CopyOutput(found)) if found == key)
        );
    }
    no_errors(&device, scope);
}

fn dynamic_group(device: &wgpu::Device, registry: &mut ResourceRegistry) -> CheckedBindings {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: false,
    });
    let key = registry
        .import_buffer(&buffer, ResourceLifetime::ImportedReadOnly, Some(0))
        .unwrap();
    let view = registry.buffer_view(key, 0..16, 0).unwrap();
    let limit = device
        .limits()
        .max_dynamic_uniform_buffers_per_pipeline_layout;
    let count = limit / 2 + 1;
    let buffers: Vec<_> = (0..count)
        .map(|binding| BufferBinding {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            view: view.clone(),
            kind: BufferBindingKind::Uniform,
            dynamic_offset: Some(0),
        })
        .collect();
    CheckedBindings::build_with_buffers(device, registry, &[], &buffers).unwrap()
}

fn invalid_copy_cases() -> [(wgpu::TextureFormat, u32, wgpu::TextureViewDimension, u32); 6] {
    [
        (
            wgpu::TextureFormat::Rgba8Unorm,
            2,
            wgpu::TextureViewDimension::D2Array,
            1,
        ),
        (
            wgpu::TextureFormat::Rgba8Unorm,
            6,
            wgpu::TextureViewDimension::Cube,
            1,
        ),
        (
            wgpu::TextureFormat::Rgba8Uint,
            1,
            wgpu::TextureViewDimension::D2,
            1,
        ),
        (
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            wgpu::TextureViewDimension::D2,
            3,
        ),
        (
            wgpu::TextureFormat::Rgba8Unorm,
            1,
            wgpu::TextureViewDimension::D2,
            0,
        ),
        (
            wgpu::TextureFormat::Depth32Float,
            1,
            wgpu::TextureViewDimension::D2,
            1,
        ),
    ]
}
