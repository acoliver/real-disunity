use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use worldrunner::compat_content::{
    AdmissionPolicy, AssemblyApiUse, CapabilityId, CapabilityRequirement, ContentReceipt,
    DependencyPolicy, ExecutionRecord, FieldSegment, GameSourceQualification, ImportProfile,
    ImportSidecar, MaterializationPlan, MaterializedScene, ResourceKind, ScriptSource,
    SelectionEffect, SelectionOperation, SelectionPolicy, TranslatedContentReceipt,
    TrustedGameSourceProof, TrustedProvider, TrustedProviderInventory, import_receipt,
};

pub const SETTINGS: &str = "ProjectSettings/GraphicsSettings.asset";
const SHADER: &str = "%YAML 1.1\n--- !u!30 &1\nGraphicsSettings:\n  m_AlwaysIncludedShaders:\n  - {fileID: 7, guid: 0000000000000000f000000000000000, type: 0}\n";
const SCENE: &str = "%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Original\n  m_Component:\n  - component: {fileID: 2}\n--- !u!4 &2\nTransform:\n  m_GameObject: {fileID: 1}\n  m_Father: {fileID: 0}\n";

pub struct Fixture(tempfile::TempDir);
impl Fixture {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Assets")).unwrap();
        fs::create_dir(root.path().join("ProjectSettings")).unwrap();
        fs::write(root.path().join("Assets/Main.unity"), SCENE).unwrap();
        fs::write(
            root.path().join("Assets/Main.unity.meta"),
            "guid: 11111111111111111111111111111111\n",
        )
        .unwrap();
        fs::write(root.path().join(SETTINGS), SHADER).unwrap();
        Self(root)
    }
    pub fn root(&self) -> &Path {
        self.0.path()
    }
    pub fn receipt(&self) -> ContentReceipt {
        import_receipt(
            self.root(),
            Path::new("Assets/Main.unity"),
            &ImportProfile::default(),
            &ImportSidecar::default(),
        )
        .unwrap()
    }
    pub fn policy(receipt: &ContentReceipt) -> SelectionPolicy {
        SelectionPolicy {
            schema_version: 1, profile_id: "trusted-protocol-microfixture-v1".into(),
            original_closure_sha256: receipt.closure_sha256().into(),
            operations: vec![SelectionOperation::ReplaceField {
                id: "explicit-unlit-selection".into(), record: ExecutionRecord::Metadata { path: SETTINGS.into() },
                source_sha256: receipt.graph().source_hashes[SETTINGS].clone(),
                field: vec![FieldSegment::Key("documents".into()), FieldSegment::Index(0), FieldSegment::Key("m_AlwaysIncludedShaders".into())],
                expected: receipt.graph().metadata_records[SETTINGS]["documents"][0]["m_AlwaysIncludedShaders"].clone(), replacement: json!([]),
                effect: SelectionEffect { behavior: "protocol-boundary unlit selection".into(), lost_effects: vec!["original pipeline rendering".into()], reason: "synthetic boundary control".into(), capability: Some(capability()) },
            }], resources: vec![], required_capabilities: vec![capability()], keyboard_only: false,
        }
    }
    pub fn select<'a>(
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
    pub fn inventory() -> TrustedProviderInventory {
        inventory(
            ResourceKind::RenderSettings,
            b"protocol-only implementation",
        )
    }
    pub fn source(receipt: &ContentReceipt, variant: u8) -> GameSourceQualification {
        GameSourceQualification::from_host(&SourceProof {
            closure: receipt.closure_sha256().into(),
            variant,
        })
        .unwrap()
    }
}
pub fn capability() -> CapabilityRequirement {
    CapabilityRequirement {
        id: CapabilityId::CanonicalUnlitQuads,
        contract_version: 1,
        resource_kind: ResourceKind::RenderSettings,
    }
}
struct Provider {
    kind: ResourceKind,
    implementation: &'static [u8],
}
impl TrustedProvider for Provider {
    fn id(&self) -> &'static str {
        "boundary-unlit"
    }
    fn resource_kind(&self) -> ResourceKind {
        self.kind
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        vec![capability()]
    }
    fn implementation_bytes(&self) -> &[u8] {
        self.implementation
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        b"protocol-only evidence"
    }
}
pub fn inventory(kind: ResourceKind, implementation: &'static [u8]) -> TrustedProviderInventory {
    let mut inventory = TrustedProviderInventory::new();
    inventory
        .register(&Provider {
            kind,
            implementation,
        })
        .unwrap();
    inventory
}
struct SourceProof {
    closure: String,
    variant: u8,
}
impl TrustedGameSourceProof for SourceProof {
    fn profile_id(&self) -> &'static str {
        "trusted-protocol-microfixture-v1"
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
        if self.variant == 0 {
            b"MZ..........................................................\x40\0\0\0PE\0\0"
        } else {
            b"MZ!.........................................................\x40\0\0\0PE\0\0"
        }
    }
    fn verification_evidence_bytes(&self) -> &[u8] {
        b"synthetic PE envelope; no compiled game or semantic inspection"
    }
    fn defined_types(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
    fn api_uses(&self) -> Vec<AssemblyApiUse> {
        vec![]
    }
}

#[derive(Default)]
pub struct Recorder {
    pub requests: Vec<Value>,
    queued: Vec<String>,
}
impl Recorder {
    pub fn queue(&mut self, effect: &str) {
        self.queued.push(effect.into());
    }
    pub fn effects(&self) -> (usize, Vec<String>) {
        (self.requests.len(), self.queued.clone())
    }
    pub fn objects(&self) -> Vec<&str> {
        self.requests
            .iter()
            .filter(|r| r["op"] == "object.create")
            .map(|r| r["name"].as_str().unwrap())
            .collect()
    }
    pub fn execute(&mut self, plan: &MaterializationPlan) -> Result<MaterializedScene, String> {
        plan.execute(|request: Value| {
            self.requests.push(request.clone());
            if request["op"] == "object.create" {
                Ok::<_, String>(json!({"handle":1,"components":[{"handle":2,"kind":"Transform"}]}))
            } else {
                Ok(json!({}))
            }
        })
        .map_err(|e| e.to_string())
    }
}
