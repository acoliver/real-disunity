use super::*;
use sha2::{Digest, Sha256};
pub(super) fn hash(value: &impl serde::Serialize) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
struct ShaderProvider {
    bytes: &'static [u8],
}
impl TrustedProvider for ShaderProvider {
    fn id(&self) -> &'static str {
        "exact-shader"
    }
    fn resource_kind(&self) -> ResourceKind {
        ResourceKind::Shader
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        vec![shader_capability()]
    }
    fn implementation_bytes(&self) -> &[u8] {
        b"boundary shader provider"
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        b"boundary exact shader bytes evidence"
    }
    fn resource_bytes(&self) -> BTreeMap<String, Vec<u8>> {
        [("unlit.shader".into(), self.bytes.to_vec())].into()
    }
}
fn shader_capability() -> CapabilityRequirement {
    CapabilityRequirement {
        id: CapabilityId::CanonicalUnlitShader,
        contract_version: 1,
        resource_kind: ResourceKind::Shader,
    }
}
#[test]
fn exact_resource_bytes_and_kind_are_bound_to_the_named_host_provider() {
    let (root, receipt, mut policy) = setup();
    policy.resources.push(SelectedResource {
        id: "unlit.shader".into(),
        resource_kind: ResourceKind::Shader,
        provider_id: "exact-shader".into(),
        capability: shader_capability(),
        bytes_sha256: format!("{:x}", Sha256::digest(b"shader v1")),
    });
    if let SelectionOperation::ReplaceField { replacement, .. } = &mut policy.operations[0] {
        *replacement = json!([{"$resource":"unlit.shader"}]);
    }
    let mut providers = inventory();
    providers
        .register(&ShaderProvider {
            bytes: b"shader v1",
        })
        .unwrap();
    let source = qualification(&receipt);
    let translated = selection(&receipt, &policy);
    let admitted = translated.admit(root.path(), &providers, &source).unwrap();
    let mut changed = inventory();
    changed
        .register(&ShaderProvider {
            bytes: b"shader v2",
        })
        .unwrap();
    assert!(
        admitted
            .verify_inputs(root.path(), &policy, &changed, &source)
            .err()
            .unwrap()
            .contains("resource bytes changed")
    );
    policy.resources[0].resource_kind = ResourceKind::Material;
    assert!(
        selection(&receipt, &policy)
            .admit(root.path(), &providers, &source)
            .is_err()
    );
}
#[test]
fn unlit_quads_capability_cannot_be_widened_to_material_or_unknown() {
    for kind in [
        ResourceKind::Material,
        ResourceKind::Shader,
        ResourceKind::Unknown,
    ] {
        let (root, receipt, mut policy) = setup();
        policy.required_capabilities[0].resource_kind = kind;
        assert!(
            selection(&receipt, &policy)
                .admit(root.path(), &inventory(), &qualification(&receipt))
                .is_err()
        );
    }
}
#[test]
fn exact_opaque_urp_record_is_unconsumed_only_in_the_new_graph() {
    let (root, _, mut policy) = setup();
    let path = "ProjectSettings/URPProjectSettings.asset";
    let bytes = "%YAML 1.1\n--- !u!114 &1\nMonoBehaviour:\n  m_Script: {fileID: 11500000, guid: 247994e1f5a72c2419c26a37e9334c01, type: 3}\n  m_LastMaterialVersion: 10\n";
    std::fs::write(root.path().join(path), bytes).unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    policy.operations.push(SelectionOperation::ExcludeRecord {
        id: "opaque-urp-settings".into(),
        record: ExecutionRecord::Metadata { path: path.into() },
        source_sha256: receipt.graph().source_hashes[path].clone(),
        expected_sha256: hash(&receipt.graph().metadata_records[path]),
        effect: SelectionEffect {
            behavior: "opaque-unconsumed".into(),
            lost_effects: vec![
                "unknown original URP project settings behavior is not executed".into(),
            ],
            reason: "exact byte-bound unconsumed record; no Editor classification".into(),
            capability: None,
        },
    });
    let translated = selection(&receipt, &policy);
    let providers = inventory();
    let admitted = translated
        .admit(root.path(), &providers, &qualification(&receipt))
        .unwrap();
    assert!(
        admitted
            .proof()
            .original_proof
            .reference_decisions
            .iter()
            .any(|d| d.observation.locator.source_path == path
                && d.classification == DependencyClassification::Blocked)
    );
    assert!(
        !admitted
            .execution_graph()
            .metadata_records
            .contains_key(path)
    );
    assert_eq!(admitted.original().sources()[path], bytes.as_bytes());
    assert!(
        admitted
            .proof()
            .ledger
            .iter()
            .any(|d| d.observation.locator.source_path == path
                && matches!(d.disposition, SelectionDisposition::ExcludedFromExecution))
    );
    std::fs::write(
        root.path().join(path),
        bytes.replace("Version: 10", "Version: 11"),
    )
    .unwrap();
    assert!(
        admitted
            .verify_inputs(root.path(), &policy, &providers, &qualification(&receipt))
            .is_err()
    );
}
#[test]
fn another_unknown_settings_record_cannot_use_the_opaque_urp_exception() {
    let (root, _, mut policy) = setup();
    let path = "ProjectSettings/Other.asset";
    std::fs::write(root.path().join(path), "%YAML 1.1\n--- !u!114 &1\nMonoBehaviour:\n  m_Script: {fileID: 11500000, guid: 247994e1f5a72c2419c26a37e9334c01, type: 3}\n").unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    policy.operations.push(SelectionOperation::ExcludeRecord {
        id: "unlisted-unknown".into(),
        record: ExecutionRecord::Metadata { path: path.into() },
        source_sha256: receipt.graph().source_hashes[path].clone(),
        expected_sha256: hash(&receipt.graph().metadata_records[path]),
        effect: SelectionEffect {
            behavior: "opaque-unconsumed".into(),
            lost_effects: vec!["unknown".into()],
            reason: "test incorrect identity".into(),
            capability: None,
        },
    });
    assert!(
        receipt
            .select_execution(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &policy
            )
            .err()
            .unwrap()
            .contains("unqualified opaque")
    );
}
