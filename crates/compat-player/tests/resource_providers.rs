#[path = "resource_providers_helpers/boundary.rs"]
mod boundary;

use compat_player::resource_providers::ResourceProviders;
use std::path::Path;
use worldrunner::{
    compat_content::TrustedProviderInventory,
    scene::gpu::{device::GpuContext, wgpu},
};
#[test]
fn actual_exact_resources_admit_only_their_named_canonical_translation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = std::env::var_os("RESOURCE_PROVIDER_ROOT").map_or(root, Into::into);
    let context = pollster::block_on(GpuContext::request_with_features(
        worldrunner::scene::post::ExactSrgb8::REQUIRED_FEATURES,
    ))
    .expect("actual GPU required, including software Vulkan");
    let mut qualified =
        ResourceProviders::qualify(&root, &context).expect("actual exact resource contracts");
    let mut inventory = TrustedProviderInventory::new();
    qualified.register(&mut inventory).unwrap();
    assert_eq!(inventory.bindings().len(), 3);
    assert_eq!(qualified.selected_resources().len(), 4);
    let before = inventory.sha256();
    assert!(qualified.register(&mut inventory).is_err());
    assert_eq!(before, inventory.sha256());
    boundary::admission(&root, &qualified, &inventory);
    let path = root.join(".work/resumed-resource-providers/actual-evidence.json");
    std::fs::write(path, qualified.evidence_bytes()).unwrap();
    let white = qualified
        .capture("canonical.unlit.material.white.v1")
        .unwrap();
    let green = qualified
        .capture("canonical.unlit.material.green-half.v1")
        .unwrap();
    assert_eq!(
        &white[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4],
        &[255; 4]
    );
    let pixel = &green[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4];
    assert_eq!(pixel, &[0, 188, 0, 255]);
    assert!(qualified.capture("arbitrary-original-material").is_err());
}

#[test]
fn missing_gpu_cannot_produce_a_qualified_composition() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::empty(),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    assert!(
        pollster::block_on(GpuContext::from_instance_with_features(
            &instance,
            wgpu::Features::empty()
        ))
        .is_err()
    );
}
