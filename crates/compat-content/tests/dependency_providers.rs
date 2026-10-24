mod dependency_support;
use dependency_support::{SETTINGS, editor_policy, project, read};
use worldrunner_compat_content::{
    AdmissionPolicy, CapabilityId, CapabilityRequirement, DependencyDisposition, Fidelity,
    ResourceKind, TrustedProvider, TrustedProviderInventory,
};

const SHADER: &str = "%YAML 1.1\n--- !u!30 &1\nGraphicsSettings:\n  m_AlwaysIncludedShaders:\n  - {fileID: 7, guid: 0000000000000000f000000000000000, type: 0}\n";
struct Provider {
    kind: ResourceKind,
    implementation: &'static [u8],
}
// This boundary double establishes inventory/admission binding only. It does not
// establish rendering or native keyboard behavior.
impl TrustedProvider for Provider {
    fn id(&self) -> &'static str {
        "test-unlit"
    }
    fn resource_kind(&self) -> ResourceKind {
        self.kind
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        if self.kind == ResourceKind::RenderSettings {
            vec![capability()]
        } else {
            vec![]
        }
    }
    fn implementation_bytes(&self) -> &[u8] {
        self.implementation
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        b"boundary-test-only"
    }
}
fn capability() -> CapabilityRequirement {
    CapabilityRequirement {
        id: CapabilityId::CanonicalUnlitQuads,
        contract_version: 1,
        resource_kind: ResourceKind::RenderSettings,
    }
}
fn setup() -> (
    tempfile::TempDir,
    worldrunner_compat_content::ContentReceipt,
    worldrunner_compat_content::DependencyPolicy,
) {
    let root = project(SHADER);
    std::fs::rename(
        root.path().join(SETTINGS),
        root.path().join("ProjectSettings/GraphicsSettings.asset"),
    )
    .unwrap();
    let receipt = read(&root);
    let observation = receipt
        .dependency_references()
        .iter()
        .find(|r| r.resource_kind == ResourceKind::Shader)
        .unwrap();
    let editor_root = project(dependency_support::EDITOR);
    let mut policy = editor_policy(&read(&editor_root));
    let rule = &mut policy.rules[0];
    rule.locator = observation.locator.clone();
    rule.source_sha256.clone_from(&observation.source_sha256);
    rule.expected_reference = observation.reference.clone().unwrap();
    rule.resource_kind = ResourceKind::Shader;
    rule.evidence[0]
        .path
        .clone_from(&observation.locator.source_path);
    rule.evidence[0]
        .sha256
        .clone_from(&observation.source_sha256);
    rule.disposition = DependencyDisposition::RenderSettingsTranslated {
        capability: capability(),
    };
    (root, receipt, policy)
}
#[test]
fn translation_requires_typed_trusted_capability_and_retains_unresolved_shader() {
    let (_root, receipt, policy) = setup();
    let mut providers = TrustedProviderInventory::new();
    assert!(
        receipt
            .admit(&AdmissionPolicy::default(), &policy, &providers)
            .is_err()
    );
    providers
        .register(&Provider {
            kind: ResourceKind::Material,
            implementation: b"material",
        })
        .unwrap();
    assert!(
        receipt
            .admit(&AdmissionPolicy::default(), &policy, &providers)
            .is_err()
    );
    let mut providers = TrustedProviderInventory::new();
    providers
        .register(&Provider {
            kind: ResourceKind::RenderSettings,
            implementation: b"first",
        })
        .unwrap();
    let admitted = receipt
        .admit(&AdmissionPolicy::default(), &policy, &providers)
        .unwrap_or_else(|e| panic!("{e}"));
    let proof = serde_json::to_value(admitted.proof()).unwrap();
    assert_eq!(proof["outcome"], "admitted_translated");
    assert!(
        proof["reference_decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["classification"] == "render_settings_translated"
                && d["observation"]["resolution"]["status"] == "unresolved")
    );
    let mut original = policy.clone();
    original.fidelity = Fidelity::Original;
    assert!(
        receipt
            .admit(&AdmissionPolicy::default(), &original, &providers)
            .is_err()
    );
}
#[test]
fn changed_provider_requires_fresh_qualification_without_freezing_inputs() {
    let (_root, receipt, policy) = setup();
    let mut providers = TrustedProviderInventory::new();
    providers
        .register(&Provider {
            kind: ResourceKind::RenderSettings,
            implementation: b"first",
        })
        .unwrap();
    let admitted = receipt
        .admit(&AdmissionPolicy::default(), &policy, &providers)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut changed = TrustedProviderInventory::new();
    changed
        .register(&Provider {
            kind: ResourceKind::RenderSettings,
            implementation: b"second",
        })
        .unwrap();
    assert_ne!(changed.sha256(), providers.sha256());
    assert!(
        receipt
            .verify_admission(
                admitted.proof(),
                &AdmissionPolicy::default(),
                &policy,
                &changed
            )
            .is_err()
    );
    assert!(
        receipt
            .admit(&AdmissionPolicy::default(), &policy, &changed)
            .is_ok()
    );
}
#[test]
fn fake_shader_to_material_builtin_mapping_cannot_supply_a_shader() {
    let (_root, receipt, mut policy) = setup();
    policy.rules[0].disposition = DependencyDisposition::ProvidedBuiltin {
        provider_id: "test-unlit".into(),
        resource_kind: ResourceKind::Material,
    };
    let mut providers = TrustedProviderInventory::new();
    providers
        .register(&Provider {
            kind: ResourceKind::Material,
            implementation: b"material",
        })
        .unwrap();
    assert!(
        receipt
            .admit(&AdmissionPolicy::default(), &policy, &providers)
            .is_err()
    );
}

#[test]
fn familiar_provider_name_cannot_admit_an_unreviewed_builtin_use_site() {
    let root = project(
        "%YAML 1.1\n--- !u!30 &1\nGraphicsSettings:\n  m_SpritesDefaultMaterial: {fileID: 9999999, guid: 0000000000000000f000000000000000, type: 0}\n",
    );
    let mut profile = worldrunner_compat_content::ImportProfile::default();
    profile.builtin_replacements.insert(
        "0000000000000000f000000000000000:9999999".into(),
        "test-unlit".into(),
    );
    let receipt = worldrunner_compat_content::import_receipt(
        root.path(),
        "Assets/Main.unity".as_ref(),
        &profile,
        &worldrunner_compat_content::ImportSidecar::default(),
    )
    .unwrap();
    let mut providers = TrustedProviderInventory::new();
    providers
        .register(&Provider {
            kind: ResourceKind::Material,
            implementation: b"material",
        })
        .unwrap();
    assert!(
        receipt
            .admit(
                &AdmissionPolicy::default(),
                &worldrunner_compat_content::DependencyPolicy::default(),
                &providers
            )
            .is_err()
    );
}
