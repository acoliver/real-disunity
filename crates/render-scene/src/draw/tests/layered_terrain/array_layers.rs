use super::*;
use worldrunner_render_gpu::configuration::ConfigurationError;
use worldrunner_render_plan::TerrainPagePolicy;

#[test]
fn caller_device_array_limit_rejects_cache_replacement_without_changing_storage() {
    use crate::terrain::pages::residency::ResidentPages;
    use worldrunner_render_gpu::device::{DeviceRequirements, request_device};
    let context = context();
    let (device, queue, _) = pollster::block_on(request_device(
        context.adapter(),
        &DeviceRequirements::default(),
    ))
    .unwrap();
    let authority = SubmissionAuthority::new(queue);
    let available = device.limits().max_texture_array_layers;
    assert_eq!(available, 256);
    assert!(context.adapter().limits().max_texture_array_layers > available);
    let scene = scene_with_slots(1);
    let settings = scene.terrain().unwrap().page_policy().unwrap().settings();
    let mut cache = ResidentPages::new(
        &device,
        &authority,
        settings,
        available / 2,
        AllocationBudget(20_000_000),
    )
    .unwrap();
    let bytes = cache.allocation_bytes(&device).unwrap();
    let slots = available / 2 + 1;
    let error = cache.replace(&device, slots).unwrap_err();
    assert!(
        matches!(error, ConfigurationError::TextureArrayLayers { required, available: limit }
        if required == u64::from(slots) * 2 && limit == available)
    );
    assert_eq!(cache.allocation_bytes(&device).unwrap(), bytes);
}

pub(super) fn scene_with_slots(slots: u32) -> RenderScene {
    let source = layered_scene(TerrainSplatFilter::Bilinear);
    let mut parts = page_source().settings.parts();
    parts.interior = [2; 2];
    parts.resident_bytes = 10_000_000;
    parts.upload_bytes_per_frame = 1024;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let policy = TerrainPagePolicy::new(settings, slots, 20_000_000).unwrap();
    let terrain = source
        .terrain()
        .unwrap()
        .clone()
        .with_page_policy(Some(policy))
        .unwrap();
    source
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}

fn policy_pixels(context: &GpuContext, scene: &RenderScene) -> Vec<u8> {
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let pipelines = ScenePipelines::new(device, COLOR_FORMAT);
    let gpu = SceneGpu::upload(device, &authority, scene, &pipelines).unwrap();
    let frame = matching_frame();
    gpu.set_frame(context.queue(), frame);
    let mut transaction = authority.begin(device).unwrap();
    gpu.prepare_terrain_pages(device, &mut transaction, &frame, [TARGET; 2])
        .unwrap();
    transaction.finish().submit().unwrap();
    assert!(gpu.terrain_pages_ready());
    draw_pixels(context, &authority, &gpu, &pipelines)
}

#[test]
fn layered_policy_above_default_array_limit_prepares_draws_and_preserves_pixels() {
    let context = context();
    assert!(context.adapter().limits().max_texture_array_layers >= 384);
    let baseline = policy_pixels(&context, &scene_with_slots(1));
    let expanded = policy_pixels(&context, &scene_with_slots(128));
    assert!(lit_pixels(&expanded) > 4000);
    assert_eq!(expanded, baseline);
}

#[test]
fn layered_policy_above_device_array_limit_fails_before_upload_with_both_counts() {
    let context = context();
    let available = context.device().limits().max_texture_array_layers;
    let slots = available / 3 + 1;
    let scene = scene_with_slots(slots);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let authority = SubmissionAuthority::new(context.queue().clone());
    let error = SceneGpu::upload(context.device(), &authority, &scene, &pipelines).unwrap_err();
    assert!(
        matches!(error, DrawError::TerrainPages(ConfigurationError::TextureArrayLayers {
        required, available: limit
    }) if required == u64::from(slots) * 3 && limit == available)
    );
    let message = error.to_string();
    assert!(message.contains(&(u64::from(slots) * 3).to_string()));
    assert!(message.contains(&available.to_string()));
}
