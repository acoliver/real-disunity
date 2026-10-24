use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use worldrunner::scene::{gpu::wgpu, post};
#[test]
fn exact_pixel_contract_rejects_every_byte_deviation() {
    let expected = [20, 188, 40, 254];
    assert!(qualify::exact_pixel(expected, expected));
    for channel in 0..4 {
        for delta in [-1_i8, 1] {
            let mut actual = expected;
            actual[channel] = if delta < 0 {
                actual[channel] - 1
            } else {
                actual[channel] + 1
            };
            assert!(
                !qualify::exact_pixel(actual, expected),
                "channel {channel}, delta {delta}"
            );
        }
    }
    assert!(!qualify::exact_pixel([1, 188, 1, 255], expected));
}
fn root() -> PathBuf {
    std::env::var_os("RESOURCE_PROVIDER_ROOT").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        PathBuf::from,
    )
}
fn context() -> GpuContext {
    pollster::block_on(GpuContext::request_with_features(
        worldrunner::scene::post::ExactSrgb8::REQUIRED_FEATURES,
    ))
    .expect("real GPU required for resource contract negatives")
}
fn readback_allocations(context: &GpuContext) -> usize {
    context
        .device()
        .generate_allocator_report()
        .expect("qualification Vulkan backend must report actual live allocations")
        .allocations
        .iter()
        .filter(|allocation| allocation.name == "post diagnostic readback")
        .count()
}
#[test]
fn released_generation_has_no_retained_host_roots() {
    let context = context();
    let probe = post::Readback::new(
        context.device(),
        post::FrameConfig::new([64, 64], gpu::FORMAT, 1),
        1_048_576,
    )
    .unwrap();
    let calibration = readback_allocations(&context);
    assert_eq!(
        calibration, 1,
        "the allocator must observe the live readback"
    );
    drop(probe);
    let binding = Binding::load(&root()).unwrap();
    let qualified = ResourceProviders::qualify(&root(), &context).unwrap();
    drop(qualified);
    let host = CompatibilityHost::boot(&binding.config()).unwrap();
    let roots = host
        .observe(&serde_json::json!({"op":"managed.statistics"}))
        .unwrap();
    assert_eq!(roots["data"]["scripts"], 0);
    assert_eq!(roots["data"]["coroutines"], 0);
    assert_eq!(roots["data"]["bindings"], 0);
    assert_eq!(roots["data"]["loaded"], false);
    assert_eq!(roots["data"]["cleanup_pending"], false);
    assert!(roots["data"]["world"].is_null());
    let native_slots = host.store_statistics().unwrap().0;
    assert_eq!(native_slots, 0);
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    let readbacks = readback_allocations(&context);
    assert_eq!(readbacks, 0);
    println!(
        "{}",
        serde_json::json!({"after_resource_services_drop":roots,
            "live_native_slots":native_slots,"live_readback_allocations":readbacks,
            "readback_allocator_calibration":calibration})
    );
}
#[test]
fn stale_compiled_tuple_is_rejected_before_provider_composition() {
    let mut stale: Vec<(&str, Vec<u8>)> = Binding::compiled_inputs()
        .map(|(relative, bytes)| (relative, bytes.to_vec()))
        .collect();
    let (changed, bytes) = stale
        .first_mut()
        .expect("image compiles resource witnesses");
    let changed = *changed;
    bytes.push(0);
    let inventory = TrustedProviderInventory::new();
    let error = Binding::load_with_inputs(
        &root(),
        stale
            .iter()
            .map(|(relative, bytes)| (*relative, bytes.as_slice())),
    )
    .err()
    .expect("stale compiled tuple must be rejected");
    assert!(error.contains("compiled resource input differs"), "{error}");
    assert!(error.ends_with(changed), "{error}");
    assert!(inventory.bindings().is_empty());
    Binding::load(&root()).expect("the image's own witnesses still qualify");
}
#[test]
fn disconnected_geometry_and_wrong_output_stage_never_qualify() {
    let context = context();
    let disconnected = qualify::run(&root(), &context, gpu::Control::Disconnected);
    assert!(
        disconnected
            .err()
            .unwrap()
            .contains("authored square geometry")
    );
    let wrong_stage = qualify::run(&root(), &context, gpu::Control::WrongStage);
    assert!(wrong_stage.err().unwrap().contains("output stage"));
}
#[test]
fn json_ready_and_unrecognized_material_fields_have_no_authority() {
    let context = context();
    let temp = tempfile::tempdir_in(root().join(".work/resumed-resource-providers")).unwrap();
    std::fs::write(
        temp.path().join("qualification.json"),
        br#"{"ready":true,"capability":"canonical_unlit_material"}"#,
    )
    .unwrap();
    assert!(ResourceProviders::qualify(temp.path(), &context).is_err());
    let mut material: serde_json::Value =
        serde_json::from_slice(catalog::material(catalog::WHITE).unwrap()).unwrap();
    material["UnityShaderExecution"] = serde_json::json!(true);
    assert!(catalog::apply(&serde_json::to_vec(&material).unwrap(), &mut []).is_err());
}
#[test]
fn private_provider_tampering_and_duplicate_last_binding_are_atomic() {
    let context = context();
    let mut qualified = ResourceProviders::qualify(&root(), &context).unwrap();
    for control in ["kind", "evidence", "implementation", "resource-id", "bytes"] {
        let mut inventory = TrustedProviderInventory::new();
        let before = inventory.sha256();
        catalog::corrupt(&mut qualified.providers[0], control);
        assert!(qualified.register(&mut inventory).is_err(), "{control}");
        assert_eq!(inventory.sha256(), before);
        qualified.providers = catalog::providers(&qualified.binding, &qualified.evidence);
    }
    let mut inventory = TrustedProviderInventory::new();
    inventory.register(&qualified.providers[2]).unwrap();
    let before = inventory.sha256();
    assert!(qualified.register(&mut inventory).is_err());
    assert_eq!(
        inventory.sha256(),
        before,
        "last binding conflict cannot partially register earlier providers"
    );
    qualified.evidence.push(0);
    assert!(
        qualified
            .register(&mut TrustedProviderInventory::new())
            .is_err()
    );
}
#[test]
fn source_shader_native_and_managed_tampering_reject_before_boot() {
    let root = root();
    let binding = Binding::load(&root).unwrap();
    let context = context();
    let temp = tempfile::tempdir_in(root.join(".work/resumed-resource-providers")).unwrap();
    for relative in binding.files().keys() {
        let target = temp.path().join(relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(root.join(relative), &target).unwrap();
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    for suffix in [
        "crates/compat-player/src/extraction.rs",
        "crates/compat-player/src/extraction/pixels.rs",
        "crates/compat-player/src/resource_providers/sprite.json",
        "crates/render-scene/src/renderer.rs",
        "crates/render-scene/src/shader/environment.wgsl",
        "crates/render-post/src/exact.rs",
        "crates/render-post/src/exact.wgsl",
        "native.so",
        "CompatibilityBridge.dll",
        "UnityCompat.dll",
    ] {
        let relative = binding
            .files()
            .keys()
            .find(|p| p.ends_with(suffix))
            .unwrap();
        let path = temp.path().join(relative);
        let original = std::fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.push(b' ');
        std::fs::write(&path, changed).unwrap();
        assert!(
            ResourceProviders::qualify(temp.path(), &context)
                .err()
                .unwrap()
                .contains("input changed"),
            "{suffix}"
        );
        std::fs::write(path, original).unwrap();
    }
}
