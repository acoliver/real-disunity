use super::*;
use crate::terrain::pages::{layered_gpu::LayeredGpuSource, residency::ResidentPages};
use worldrunner_render_gpu::{
    configuration::{ConfigurationError, DriverAllocationError},
    device::{DeviceRequirements, request_device},
};

fn shared_array_scene() -> RenderScene {
    let scene = array_layers::scene_with_slots(1);
    let resources = SceneResources::new(
        (0..4)
            .map(|value| texels([1; 2], &[value, 128, 255, 255]))
            .collect(),
        vec![material(SceneAlphaMode::Opaque, false)],
    )
    .unwrap();
    scene
        .with_resources(resources, SceneGeometry::new(vec![], vec![]).unwrap(), None)
        .unwrap()
}

fn limited_device(context: &GpuContext) -> (wgpu::Device, SubmissionAuthority) {
    let requirements = DeviceRequirements {
        required_limits: wgpu::Limits {
            max_texture_array_layers: 3,
            ..context.device().limits()
        },
        ..Default::default()
    };
    let (device, queue, _) =
        pollster::block_on(request_device(context.adapter(), &requirements)).unwrap();
    (device, SubmissionAuthority::new(queue))
}

fn assert_source_limit(error: &ConfigurationError<DriverAllocationError>) {
    assert!(
        matches!(
            error,
            ConfigurationError::TextureArrayLayers {
                required: 4,
                available: 3
            }
        ),
        "{error:?}"
    );
}

#[test]
fn shared_source_array_limit_is_typed_before_scene_upload() {
    let context = context();
    let (device, authority) = limited_device(&context);
    let pipelines = ScenePipelines::new(&device, COLOR_FORMAT);
    let error =
        SceneGpu::upload(&device, &authority, &shared_array_scene(), &pipelines).unwrap_err();
    let DrawError::TerrainPages(error) = error else {
        panic!("expected typed terrain array limit, got {error:?}");
    };
    assert_source_limit(&error);
}

#[test]
fn shared_source_array_limit_is_typed_for_cache_creation_and_replacement() {
    let context = context();
    let (device, authority) = limited_device(&context);
    let scene = shared_array_scene();
    let source = LayeredGpuSource::new(scene.terrain().unwrap(), scene.resources()).unwrap();
    let settings = scene.terrain().unwrap().page_policy().unwrap().settings();
    let result = ResidentPages::new_layered(
        &device,
        &authority,
        settings,
        1,
        AllocationBudget(20_000_000),
        &source,
    );
    assert_source_limit(&result.err().expect("four source layers exceed three"));
    let full_authority = SubmissionAuthority::new(context.queue().clone());
    let mut cache = ResidentPages::new_layered(
        context.device(),
        &full_authority,
        settings,
        1,
        AllocationBudget(20_000_000),
        &source,
    )
    .unwrap();
    let bytes = cache.allocation_bytes(context.device()).unwrap();
    assert_source_limit(&cache.replace(&device, 1).unwrap_err());
    assert_eq!(cache.allocation_bytes(context.device()).unwrap(), bytes);
}
