use compat_player::trusted::prepare_counter;
use serde_json::json;
use std::path::{Path, PathBuf};
use worldrunner::{
    compat_content::{
        AdmissionPolicy, CapabilityId, CapabilityRequirement, ComponentProjection, ContentReceipt,
        DependencyPolicy, ExecutionRecord, FieldSegment, ImportProfile, ImportSidecar,
        ResourceKind, SelectionEffect, SelectionOperation, SelectionPolicy,
        TranslatedContentReceipt, import_receipt,
    },
    managed::{CompatibilityHost, ManagedHostConfig},
};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| {
            p.join(".work/resumed-finish-trusted/fixture/Assets/Main.unity")
                .is_file()
        })
        .expect("counter fixture workspace")
        .to_path_buf()
}

fn receipt(project: &Path) -> ContentReceipt {
    let mut import = ImportProfile::default();
    import.script_types.insert(
        "22222222222222222222222222222222".into(),
        "TrustedCounter".into(),
    );
    import_receipt(
        project,
        Path::new("Assets/Main.unity"),
        &import,
        &ImportSidecar::default(),
    )
    .expect("source-bound counter receipt")
}

fn translated(receipt: &ContentReceipt) -> TranslatedContentReceipt<'_> {
    let counter = receipt
        .graph()
        .components
        .iter()
        .find(|c| {
            c.script
                .as_ref()
                .is_some_and(|s| s.type_name == "TrustedCounter")
        })
        .expect("counter script component");
    let capability = CapabilityRequirement {
        id: CapabilityId::MappedTileType,
        contract_version: 1,
        resource_kind: ResourceKind::Script,
    };
    let selected = SelectionOperation::ReplaceField {
        id: "counter-source-map".into(),
        record: ExecutionRecord::Component {
            source: counter.source.clone(),
            projection: ComponentProjection::Fields,
        },
        source_sha256: receipt.graph().source_hashes["Assets/Main.unity"].clone(),
        field: vec![FieldSegment::Key("m_Script".into())],
        expected: counter.fields["m_Script"].clone(),
        replacement: counter.fields["m_Script"].clone(),
        effect: SelectionEffect {
            behavior: "bind only the verified counter type".into(),
            lost_effects: vec!["other game scripts".into()],
            reason: "finite source-bound managed counter fixture".into(),
            capability: Some(capability.clone()),
        },
    };
    receipt
        .select_execution(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &SelectionPolicy {
                schema_version: 1,
                profile_id: "trusted-counter-v1".into(),
                original_closure_sha256: receipt.closure_sha256().into(),
                operations: vec![selected],
                resources: vec![],
                required_capabilities: vec![capability],
                keyboard_only: false,
            },
        )
        .expect("counter-only translation receipt")
}

fn host(root: &Path) -> CompatibilityHost {
    // Native and bridge come from the same frozen generation the counter PE references.
    let native = root.join(".work/resumed-idle-qualification/02/native.so");
    let bridge = root.join(".work/resumed-idle-qualification/02/managed/artifacts/bin/CompatibilityBridge/debug/CompatibilityBridge.dll");
    CompatibilityHost::boot(&ManagedHostConfig {
        ffi_library: native,
        runtime_config: bridge.with_extension("runtimeconfig.json"),
        assembly: bridge,
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    })
    .expect("actual frozen compatibility host")
}

#[test]
fn contained_counter_loads_updates_and_unloads_on_the_same_host() {
    let root = workspace();
    let project = root.join(".work/resumed-finish-trusted/fixture");
    let receipt = receipt(&project);
    let translated = translated(&receipt);
    let mut host = host(&root);
    let mut loaded = prepare_counter(&project, &translated, &mut host)
        .expect("guarded counter prepare")
        .load()
        .expect("actual counter load");
    assert_eq!(loaded.update().expect("managed Update"), 1);
    assert_eq!(loaded.update().expect("second managed Update"), 2);
    loaded
        .unload()
        .expect("counter unload and world destruction");
    assert_eq!(
        host.store_statistics()
            .expect("native cleanup statistics")
            .0,
        0
    );
    let stats = host
        .observe(&json!({"op":"managed.statistics"}))
        .expect("managed cleanup statistics");
    for key in ["scripts", "coroutines", "bindings"] {
        assert_eq!(stats["data"][key], 0);
    }
}
