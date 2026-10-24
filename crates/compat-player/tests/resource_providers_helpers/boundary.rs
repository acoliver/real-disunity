use super::ResourceProviders;
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
use worldrunner::compat_content::*;

// This proof isolates the public content boundary. It establishes no PE/CLR gameplay.
struct BoundarySource {
    closure: String,
}
impl TrustedGameSourceProof for BoundarySource {
    fn profile_id(&self) -> &'static str {
        "resource-microfixture-v1"
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
        b"synthetic source proof for resource admission boundary only"
    }
    fn defined_types(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
    fn api_uses(&self) -> Vec<AssemblyApiUse> {
        vec![]
    }
}
fn fixture(root: &Path) -> (tempfile::TempDir, ContentReceipt) {
    let temp = tempfile::tempdir_in(root.join(".work/resumed-resource-providers")).unwrap();
    std::fs::create_dir(temp.path().join("Assets")).unwrap();
    std::fs::create_dir(temp.path().join("ProjectSettings")).unwrap();
    std::fs::write(temp.path().join("Assets/Main.unity"), "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: ResourceBoundary\n  m_Component:\n  - component: {fileID: 2}\n--- !u!4 &2\nTransform:\n  m_GameObject: {fileID: 1}\n  m_Father: {fileID: 0}\n").unwrap();
    std::fs::write(
        temp.path().join("Assets/Main.unity.meta"),
        "guid: 11111111111111111111111111111111\n",
    )
    .unwrap();
    std::fs::write(temp.path().join("ProjectSettings/GraphicsSettings.asset"), "%YAML 1.1\n--- !u!30 &1\nGraphicsSettings:\n  m_AlwaysIncludedShaders:\n  - {fileID: 7, guid: 0000000000000000f000000000000000, type: 0}\n").unwrap();
    let receipt = import_receipt(
        temp.path(),
        Path::new("Assets/Main.unity"),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    (temp, receipt)
}
fn policy(receipt: &ContentReceipt, resources: Vec<SelectedResource>) -> SelectionPolicy {
    let shader = resources
        .iter()
        .find(|r| r.resource_kind == ResourceKind::Shader)
        .unwrap();
    let path = "ProjectSettings/GraphicsSettings.asset";
    SelectionPolicy {
        schema_version: 1,
        profile_id: "resource-microfixture-v1".into(),
        original_closure_sha256: receipt.closure_sha256().into(),
        operations: vec![SelectionOperation::ReplaceField {
            id: "exact-canonical-shader".into(),
            record: ExecutionRecord::Metadata { path: path.into() },
            source_sha256: receipt.graph().source_hashes[path].clone(),
            field: vec![
                FieldSegment::Key("documents".into()),
                FieldSegment::Index(0),
                FieldSegment::Key("m_AlwaysIncludedShaders".into()),
            ],
            expected:
                receipt.graph().metadata_records[path]["documents"][0]["m_AlwaysIncludedShaders"]
                    .clone(),
            replacement: json!([{"$resource":shader.id}]),
            effect: SelectionEffect {
                behavior: "canonical unlit resource".into(),
                lost_effects: vec!["original shader and material lighting/URP semantics".into()],
                reason: "exact resource boundary fixture".into(),
                capability: Some(shader.capability.clone()),
            },
        }],
        required_capabilities: resources.iter().map(|r| r.capability.clone()).collect(),
        resources,
        keyboard_only: false,
    }
}
pub(super) fn admission(
    root: &Path,
    qualified: &ResourceProviders<'_>,
    inventory: &TrustedProviderInventory,
) {
    let (temp, receipt) = fixture(root);
    let policy = policy(&receipt, qualified.selected_resources());
    let source = GameSourceQualification::from_host(&BoundarySource {
        closure: receipt.closure_sha256().into(),
    })
    .unwrap();
    let selected = receipt
        .select_execution(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &policy,
        )
        .unwrap();
    assert!(
        selected
            .admit(temp.path(), &TrustedProviderInventory::new(), &source)
            .is_err()
    );
    let admitted = selected.admit(temp.path(), inventory, &source).unwrap();
    admitted
        .verify_inputs(temp.path(), &policy, inventory, &source)
        .unwrap();
    assert_eq!(
        admitted.proof().original_proof.outcome,
        AdmissionOutcome::Blocked
    );
    for change in ["kind", "id", "bytes", "font", "tile", "duplicate"] {
        let mut changed = policy.clone();
        match change {
            "kind" => changed.resources[0].resource_kind = ResourceKind::Shader,
            "id" => changed.resources[0].id = "unqualified-original-material".into(),
            "bytes" => changed.resources[0].bytes_sha256 = "00".repeat(32),
            "font" => changed.required_capabilities.push(CapabilityRequirement {
                id: CapabilityId::MappedFont,
                contract_version: 1,
                resource_kind: ResourceKind::Font,
            }),
            "tile" => changed.required_capabilities.push(CapabilityRequirement {
                id: CapabilityId::MappedTileType,
                contract_version: 1,
                resource_kind: ResourceKind::Script,
            }),
            "duplicate" => changed.resources.push(changed.resources[0].clone()),
            _ => unreachable!(),
        }
        let selected = receipt
            .select_execution(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &changed,
            )
            .unwrap();
        assert!(
            selected.admit(temp.path(), inventory, &source).is_err(),
            "{change}"
        );
    }
}
