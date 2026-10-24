mod dependency_support;
use dependency_support::{project, read};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
use worldrunner_compat_content::*;

const SETTINGS: &str = "ProjectSettings/GraphicsSettings.asset";
const SHADER: &str = "%YAML 1.1\n--- !u!30 &1\nGraphicsSettings:\n  m_AlwaysIncludedShaders:\n  - {fileID: 7, guid: 0000000000000000f000000000000000, type: 0}\n";
fn capability() -> CapabilityRequirement {
    CapabilityRequirement {
        id: CapabilityId::CanonicalUnlitQuads,
        contract_version: 1,
        resource_kind: ResourceKind::RenderSettings,
    }
}
struct Provider;
// These doubles qualify the content request boundary only, with no renderer,
// keyboard, PE inspector or real-game execution behavior claimed.
impl TrustedProvider for Provider {
    fn id(&self) -> &'static str {
        "boundary-unlit"
    }
    fn resource_kind(&self) -> ResourceKind {
        ResourceKind::RenderSettings
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        vec![capability()]
    }
    fn implementation_bytes(&self) -> &[u8] {
        b"boundary implementation"
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        b"boundary evidence"
    }
}
struct SourceProof {
    closure: String,
}
impl TrustedGameSourceProof for SourceProof {
    fn profile_id(&self) -> &'static str {
        "micro-unlit-v1"
    }
    fn content_closure_sha256(&self) -> &str {
        &self.closure
    }
    fn source_directory(&self) -> &Path {
        Path::new("Assets")
    }
    fn sources(&self) -> Vec<ScriptSource> {
        vec![]
    }
    fn assembly_bytes(&self) -> &[u8] {
        b"MZ..........................................................\x40\0\0\0PE\0\0"
    }
    fn verification_evidence_bytes(&self) -> &[u8] {
        b"synthetic boundary proof, no game code"
    }
    fn defined_types(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
    fn api_uses(&self) -> Vec<AssemblyApiUse> {
        vec![]
    }
}
fn setup() -> (tempfile::TempDir, ContentReceipt, SelectionPolicy) {
    let root = project(SHADER);
    std::fs::rename(
        root.path().join(dependency_support::SETTINGS),
        root.path().join(SETTINGS),
    )
    .unwrap();
    let receipt = read(&root);
    let policy = SelectionPolicy {
        schema_version: 1, profile_id: "micro-unlit-v1".into(),
        original_closure_sha256: receipt.closure_sha256().into(),
        operations: vec![SelectionOperation::ReplaceField {
            id: "select-canonical-unlit-shader-list".into(),
            record: ExecutionRecord::Metadata { path: SETTINGS.into() },
            source_sha256: receipt.graph().source_hashes[SETTINGS].clone(),
            field: vec![FieldSegment::Key("documents".into()), FieldSegment::Index(0), FieldSegment::Key("m_AlwaysIncludedShaders".into())],
            expected: receipt.graph().metadata_records[SETTINGS]["documents"][0]["m_AlwaysIncludedShaders"].clone(),
            replacement: json!([]),
            effect: SelectionEffect { behavior: "canonical unlit quads".into(), lost_effects: vec!["original shader compilation and lighting".into()], reason: "exact boundary translation".into(), capability: Some(capability()) },
        }], resources: vec![], required_capabilities: vec![capability()], keyboard_only: false,
    };
    (root, receipt, policy)
}
#[test]
fn exact_translation_admits_separate_graph_while_original_stays_blocked() {
    let (root, receipt, policy) = setup();
    let mut providers = TrustedProviderInventory::new();
    providers.register(&Provider).unwrap();
    let original = receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &providers,
        )
        .err()
        .unwrap();
    assert_eq!(original.proof.outcome, AdmissionOutcome::Blocked);
    let translated = receipt
        .select_execution(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &policy,
        )
        .unwrap();
    let source = GameSourceQualification::from_host(&SourceProof {
        closure: receipt.closure_sha256().into(),
    })
    .unwrap();
    let admitted = translated.admit(root.path(), &providers, &source).unwrap();
    assert_eq!(
        serde_json::to_value(&admitted.proof().original_proof).unwrap(),
        serde_json::to_value(original.proof).unwrap()
    );
    assert_eq!(
        admitted.proof().ledger.len(),
        receipt.dependency_references().len()
    );
    assert_eq!(
        admitted.execution_graph().metadata_records[SETTINGS]["documents"][0]["m_AlwaysIncludedShaders"],
        json!([])
    );
    assert_ne!(
        serde_json::to_value(admitted.execution_graph()).unwrap(),
        serde_json::to_value(receipt.graph()).unwrap()
    );
    admitted
        .verify_inputs(root.path(), &policy, &providers, &source)
        .unwrap();
    assert!(
        receipt
            .admit(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &providers
            )
            .is_err()
    );
}
fn inventory() -> TrustedProviderInventory {
    let mut providers = TrustedProviderInventory::new();
    providers.register(&Provider).unwrap();
    providers
}
fn qualification(receipt: &ContentReceipt) -> GameSourceQualification {
    GameSourceQualification::from_host(&SourceProof {
        closure: receipt.closure_sha256().into(),
    })
    .unwrap()
}
fn selection<'a>(
    receipt: &'a ContentReceipt,
    policy: &SelectionPolicy,
) -> TranslatedContentReceipt<'a> {
    receipt
        .select_execution(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            policy,
        )
        .unwrap()
}
#[test]
fn original_editor_classification_does_not_authorize_unreviewed_translation() {
    let root = project(dependency_support::EDITOR);
    let receipt = read(&root);
    let reviewed = dependency_support::editor_policy(&receipt);
    assert!(
        receipt
            .admit(&AdmissionPolicy::default(), &reviewed, &inventory())
            .is_ok()
    );
    let mut original = reviewed;
    original.fidelity = Fidelity::Original;
    assert!(
        receipt
            .admit(&AdmissionPolicy::default(), &original, &inventory())
            .is_err()
    );
}
#[test]
fn exact_old_value_and_source_byte_preconditions_are_required() {
    let (_, receipt, mut policy) = setup();
    if let SelectionOperation::ReplaceField { expected, .. } = &mut policy.operations[0] {
        *expected = json!(["wrong"]);
    }
    assert!(
        receipt
            .select_execution(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &policy
            )
            .err()
            .unwrap()
            .contains("old-value")
    );
    let (_, receipt, mut policy) = setup();
    if let SelectionOperation::ReplaceField { source_sha256, .. } = &mut policy.operations[0] {
        *source_sha256 = "0".repeat(64);
    }
    assert!(
        receipt
            .select_execution(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &policy
            )
            .err()
            .unwrap()
            .contains("source byte")
    );
}
#[test]
fn no_actual_provider_cannot_admit_the_selected_graph() {
    let (root, receipt, policy) = setup();
    let translated = selection(&receipt, &policy);
    assert!(
        translated
            .admit(
                root.path(),
                &TrustedProviderInventory::new(),
                &qualification(&receipt)
            )
            .err()
            .unwrap()
            .contains("provider missing")
    );
}
#[test]
fn current_input_and_policy_changes_invalidate_opaque_admission() {
    let (root, receipt, mut policy) = setup();
    let translated = selection(&receipt, &policy);
    let providers = inventory();
    let source = qualification(&receipt);
    let admitted = translated.admit(root.path(), &providers, &source).unwrap();
    policy.profile_id.push_str("-changed");
    assert!(
        admitted
            .verify_inputs(root.path(), &policy, &providers, &source)
            .err()
            .unwrap()
            .contains("policy changed")
    );
    std::fs::write(
        root.path().join(SETTINGS),
        SHADER.replace("fileID: 7", "fileID: 8"),
    )
    .unwrap();
    assert!(
        admitted
            .verify_inputs(
                root.path(),
                translated.selection_policy(),
                &providers,
                &source
            )
            .is_err()
    );
}
fn reseal(proof: serde_json::Value) -> TranslatedAdmissionProof {
    use sha2::{Digest, Sha256};
    let mut proof: TranslatedAdmissionProof = serde_json::from_value(proof).unwrap();
    proof.admission_sha256.clear();
    proof.admission_sha256 = format!("{:x}", Sha256::digest(serde_json::to_vec(&proof).unwrap()));
    TranslatedAdmissionProof::from_json(&serde_json::to_vec(&proof).unwrap()).unwrap()
}
#[test]
fn correctly_resealed_removed_original_observation_and_ledger_are_rejected() {
    let (root, receipt, policy) = setup();
    let translated = selection(&receipt, &policy);
    let providers = inventory();
    let source = qualification(&receipt);
    let admitted = translated.admit(root.path(), &providers, &source).unwrap();
    for field in ["ledger", "original_proof"] {
        let mut value = serde_json::to_value(admitted.proof()).unwrap();
        if field == "ledger" {
            value[field].as_array_mut().unwrap().pop();
        } else {
            value[field]["reference_decisions"]
                .as_array_mut()
                .unwrap()
                .pop();
        }
        let forged = reseal(value);
        assert!(
            translated
                .verify_admission(root.path(), &forged, &providers, &source)
                .is_err()
        );
    }
}
#[test]
fn introduced_nested_markers_and_new_incoming_edges_block() {
    for injected in [
        json!({"outer":{"$missing":{"guid":"unlisted","file_id":9}}}),
        json!({"outer":{"$ref":"11111111111111111111111111111111:999","guid":"11111111111111111111111111111111","file_id":999}}),
        json!({"outer":{"$ref":"11111111111111111111111111111111:2","guid":"11111111111111111111111111111111","file_id":1}}),
    ] {
        let (root, receipt, mut policy) = setup();
        if let SelectionOperation::ReplaceField { replacement, .. } = &mut policy.operations[0] {
            *replacement = injected;
        }
        let translated = selection(&receipt, &policy);
        assert!(
            translated
                .admit(root.path(), &inventory(), &qualification(&receipt))
                .is_err()
        );
    }
}
#[test]
fn null_default_pipeline_does_not_exclude_any_quality_tier() {
    let (root, _, _) = setup();
    let quality = "%YAML 1.1\n--- !u!47 &1\nQualitySettings:\n  m_QualitySettings:\n  - name: Low\n    customRenderPipeline: {fileID: 0}\n  - name: High\n    customRenderPipeline: {fileID: 11400000, guid: 22222222222222222222222222222222, type: 2}\n";
    std::fs::write(
        root.path().join("ProjectSettings/QualitySettings.asset"),
        quality,
    )
    .unwrap();
    let receipt = read(&root);
    let (_, _, mut policy) = setup();
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    let translated = selection(&receipt, &policy);
    assert!(
        translated
            .admit(root.path(), &inventory(), &qualification(&receipt))
            .is_err()
    );
}
#[test]
fn empty_keyboard_proof_cannot_authorize_importer_exclusion() {
    let (root, receipt, mut policy) = setup();
    policy.keyboard_only = true;
    let translated = selection(&receipt, &policy);
    assert!(
        translated
            .admit(root.path(), &inventory(), &qualification(&receipt))
            .err()
            .unwrap()
            .contains("complete typed GameSource")
    );
}

#[path = "translated_execution/expanded.rs"]
mod expanded;
#[path = "translated_execution/proof_controls.rs"]
mod proof_controls;
#[path = "translated_execution/resources.rs"]
mod resources;
#[path = "translated_execution/source_controls.rs"]
mod source_controls;
#[path = "translated_execution/value_controls.rs"]
mod value_controls;
