use std::path::Path;

use worldrunner_disunity_runtime::{CompatibilityHost, HostError, ManagedHostConfig};

#[test]
fn compatibility_host_rejects_missing_artifacts_without_retaining_the_host_claim() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = ManagedHostConfig {
        ffi_library: root.join("missing-native"),
        runtime_config: root.join("missing-runtimeconfig.json"),
        assembly: root.join("missing-compatibility.dll"),
        bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
    };
    for _ in 0..2 {
        assert!(matches!(
            CompatibilityHost::boot(&config),
            Err(HostError::ManagedArtifactsMissing(_))
        ));
    }
}
