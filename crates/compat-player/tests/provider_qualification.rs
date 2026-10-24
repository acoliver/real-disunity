#[path = "provider_qualification/controls.rs"]
mod controls;
#[path = "../src"]
mod current {
    pub(crate) mod extraction;
}
use current::extraction;
#[path = "provider_qualification/identity.rs"]
mod identity;
#[path = "../src/providers.rs"]
mod providers;
#[path = "../src/resources.rs"]
mod resources;
#[path = "provider_qualification/source.rs"]
mod source;
#[path = "../src/tiles.rs"]
mod tiles;

use std::path::Path;
use worldrunner::{
    compat_content::{CapabilityId, ResourceKind, TrustedProviderInventory},
    scene::gpu::{device::GpuContext, wgpu},
};

fn required_gpu() -> GpuContext {
    if std::env::var_os("PROVIDER_NO_GPU").is_some() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::empty(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        return pollster::block_on(GpuContext::from_instance_with_features(
            &instance,
            wgpu::Features::empty(),
        ))
        .expect("required actual GPU is unavailable");
    }
    pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("required actual GPU")
}

#[test]
fn actual_host_composition_qualifies_only_bounded_input_and_unlit_contracts() {
    let root = Path::new(
        option_env!("PROVIDER_QUALIFICATION_ROOT")
            .unwrap_or(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
    )
    .canonicalize()
    .unwrap();
    let context = required_gpu();
    let mut qualified =
        providers::qualify(&root, &context).expect("real provider qualification must execute");
    let mut inventory = TrustedProviderInventory::new();
    qualified.register(&mut inventory).unwrap();
    assert_eq!(inventory.bindings().len(), 2);
    for provider in inventory.bindings().values() {
        assert!(!provider.implementation_sha256.is_empty());
        for capability in &provider.capabilities {
            match capability.id {
                CapabilityId::CanonicalKeyboardSnapshot => {
                    assert_eq!(capability.resource_kind, ResourceKind::Input);
                }
                CapabilityId::CanonicalUnlitQuads => {
                    assert_eq!(capability.resource_kind, ResourceKind::RenderSettings);
                }
                _ => panic!("unqualified capability was registered"),
            }
        }
    }
    let before = inventory.sha256();
    assert!(qualified.register(&mut inventory).is_err());
    assert_eq!(before, inventory.sha256());
    std::fs::write(
        root.join(".work/resumed-provider-qualification-v2/actual-evidence.json"),
        qualified.evidence_bytes(),
    )
    .unwrap();
    assert_eq!(qualified.host_mut().store_statistics().unwrap().0, 0);
    source::actual_pe_source_proof(&root, &qualified);
    controls::wrong_kind(&root, &qualified);
    controls::render_failures(&root, &context, &mut qualified);
    identity::change_after_qualification_is_rejected(&root, &qualified);
    identity::altered_tuple_is_rejected(&root, &context);
}

#[test]
fn fixture_json_cannot_supply_a_qualification_seal() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("qualification.json"),
        br#"{"ready":true,"native":"any","evidence":"pass"}"#,
    )
    .unwrap();
    let context = pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty()))
        .expect("required actual GPU");
    assert!(providers::qualify(root.path(), &context).is_err());
}
