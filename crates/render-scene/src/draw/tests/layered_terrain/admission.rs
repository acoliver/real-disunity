use super::*;
use crate::terrain::pages::layered_gpu::{LayeredGpuError, LayeredGpuSource};
use worldrunner_render_post::contracts::ResourceRegistry;

fn limited_device(limits: wgpu::Limits) -> (wgpu::Device, wgpu::Queue) {
    let context = context();
    pollster::block_on(context.adapter().request_device(&wgpu::DeviceDescriptor {
        required_features: crate::device::REQUIRED_FEATURES,
        required_limits: limits,
        ..Default::default()
    }))
    .unwrap()
}

#[test]
fn two_storage_bindings_preserve_linear_scene_and_reject_layered_upload() {
    let limits = wgpu::Limits {
        max_storage_buffers_per_shader_stage: 2,
        ..Default::default()
    };
    let (device, queue) = limited_device(limits);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pipelines = ScenePipelines::new(&device, COLOR_FORMAT);
    assert!(pipelines.layered.is_none());
    let authority = SubmissionAuthority::new(queue);
    let linear = super::super::terrain_scene(1);
    assert!(SceneGpu::upload(&device, &authority, &linear, &pipelines).is_ok());
    let scene = layered_scene(TerrainSplatFilter::Bilinear);
    assert!(matches!(
        SceneGpu::upload(&device, &authority, &scene, &pipelines),
        Err(DrawError::LayeredGpu(LayeredGpuError::Unsupported))
    ));
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn source_binding_size_rejection_precedes_gpu_allocations_and_validation() {
    let limits = wgpu::Limits {
        max_storage_buffer_binding_size: 128,
        ..Default::default()
    };
    let (device, queue) = limited_device(limits);
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pipelines = ScenePipelines::new(&device, COLOR_FORMAT);
    let authority = SubmissionAuthority::new(queue);
    let scene = layered_scene(TerrainSplatFilter::Bilinear);
    assert!(matches!(
        SceneGpu::upload(&device, &authority, &scene, &pipelines),
        Err(DrawError::LayeredGpu(LayeredGpuError::BufferLimit {
            buffer: "parameters",
            required: 192,
            limit: 128
        }))
    ));
    let source = LayeredGpuSource::new(scene.terrain().unwrap(), scene.resources()).unwrap();
    assert!(matches!(
        source.operations(&device, &mut ResourceRegistry::default(), &[[0.0; 2]]),
        Err(LayeredGpuError::BufferLimit {
            required: 192,
            limit: 128,
            ..
        })
    ));
    assert!(pollster::block_on(scope.pop()).is_none());
}
