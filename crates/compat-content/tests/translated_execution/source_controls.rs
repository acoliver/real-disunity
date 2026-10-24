use super::*;
struct KeyboardProof {
    base: SourceProof,
    sources: Vec<ScriptSource>,
    api: AssemblyApiUse,
}
impl TrustedGameSourceProof for KeyboardProof {
    fn profile_id(&self) -> &str {
        self.base.profile_id()
    }
    fn content_closure_sha256(&self) -> &str {
        &self.base.closure
    }
    fn source_directory(&self) -> &Path {
        Path::new("Assets")
    }
    fn sources(&self) -> Vec<ScriptSource> {
        self.sources.clone()
    }
    fn assembly_bytes(&self) -> &[u8] {
        self.base.assembly_bytes()
    }
    fn verification_evidence_bytes(&self) -> &[u8] {
        b"boundary-only typed API closure"
    }
    fn defined_types(&self) -> BTreeMap<String, String> {
        [("Control".into(), "Assets/Control.cs".into())].into()
    }
    fn api_uses(&self) -> Vec<AssemblyApiUse> {
        vec![self.api.clone()]
    }
}
struct InputProvider;
impl TrustedProvider for InputProvider {
    fn id(&self) -> &'static str {
        "boundary-keyboard"
    }
    fn resource_kind(&self) -> ResourceKind {
        ResourceKind::Input
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        vec![input_capability()]
    }
    fn implementation_bytes(&self) -> &[u8] {
        b"boundary snapshot input implementation"
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        b"boundary snapshot input evidence"
    }
}
fn input_capability() -> CapabilityRequirement {
    CapabilityRequirement {
        id: CapabilityId::CanonicalKeyboardSnapshot,
        contract_version: 1,
        resource_kind: ResourceKind::Input,
    }
}
fn keyboard_host(root: &Path, receipt: &ContentReceipt) -> KeyboardProof {
    KeyboardProof {
        base: SourceProof {
            closure: receipt.closure_sha256().into(),
        },
        sources: script_census(root, Path::new("Assets"), &ImportProfile::default()).unwrap(),
        api: AssemblyApiUse {
            assembly: "UnityCompat".into(),
            type_name: "UnityEngine.InputSystem.Keyboard".into(),
            member: "get_current".into(),
            surface: ApiSurface::KeyboardSnapshot,
        },
    }
}
#[test]
fn keyboard_api_closure_rejects_action_wrappers_and_misclassified_api_uses() {
    let (root, _, mut policy) = setup();
    std::fs::write(
        root.path().join("Assets/Control.cs"),
        "public class Control {}\n",
    )
    .unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    policy.keyboard_only = true;
    policy.required_capabilities.push(input_capability());
    let translated = selection(&receipt, &policy);
    let mut providers = inventory();
    providers.register(&InputProvider).unwrap();
    let mut host = keyboard_host(root.path(), &receipt);
    let source = GameSourceQualification::from_host(&host).unwrap();
    let admitted = translated.admit(root.path(), &providers, &source).unwrap();
    for (kind, name) in [
        (
            ApiSurface::InputActions,
            "UnityEngine.InputSystem.InputAction",
        ),
        (ApiSurface::CanonicalFacade, "GeneratedInputActions"),
        (
            ApiSurface::KeyboardSnapshot,
            "UnityEngine.InputSystem.InputAction",
        ),
    ] {
        host.api.surface = kind;
        host.api.type_name = name.into();
        let unqualified = GameSourceQualification::from_host(&host).unwrap();
        assert!(
            translated
                .admit(root.path(), &providers, &unqualified)
                .err()
                .unwrap()
                .contains("action/wrapper")
        );
    }
    std::fs::write(
        root.path().join("Assets/NewWrapper.cs"),
        "public class NewWrapper {}\n",
    )
    .unwrap();
    assert!(
        admitted
            .verify_inputs(root.path(), &policy, &providers, &source)
            .err()
            .unwrap()
            .contains("census changed")
    );
}
#[test]
fn changed_source_qualification_or_profile_is_not_reusable() {
    let (root, receipt, policy) = setup();
    let translated = selection(&receipt, &policy);
    let source = qualification(&receipt);
    let providers = inventory();
    let admitted = translated.admit(root.path(), &providers, &source).unwrap();
    let changed = GameSourceQualification::from_host(&SourceProof {
        closure: "0".repeat(64),
    })
    .unwrap();
    assert!(
        admitted
            .verify_inputs(root.path(), &policy, &providers, &changed)
            .err()
            .unwrap()
            .contains("closure mismatch")
    );
}

#[test]
fn qualified_managed_type_cannot_supply_a_shader_use_site() {
    let (root, _, mut policy) = setup();
    std::fs::write(
        root.path().join("Assets/Control.cs"),
        "public class Control {}\n",
    )
    .unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    if let SelectionOperation::ReplaceField { replacement, .. } = &mut policy.operations[0] {
        *replacement = json!([{"$type":"Control"}]);
    }
    let source = GameSourceQualification::from_host(&keyboard_host(root.path(), &receipt)).unwrap();
    assert!(
        selection(&receipt, &policy)
            .admit(root.path(), &inventory(), &source)
            .is_err()
    );
}

#[test]
fn actual_facade_keycontrol_namespace_is_accepted_and_absent_namespace_blocks() {
    let (root, _, mut policy) = setup();
    std::fs::write(
        root.path().join("Assets/Control.cs"),
        "public class Control {}\n",
    )
    .unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    policy.keyboard_only = true;
    policy.required_capabilities.push(input_capability());
    let translated = selection(&receipt, &policy);
    let mut providers = inventory();
    providers.register(&InputProvider).unwrap();
    let mut host = keyboard_host(root.path(), &receipt);
    host.api.type_name = "UnityEngine.InputSystem.KeyControl".into();
    host.api.member = "get_isPressed".into();
    let source = GameSourceQualification::from_host(&host).unwrap();
    assert!(translated.admit(root.path(), &providers, &source).is_ok());
    host.api.type_name = "UnityEngine.InputSystem.Controls.KeyControl".into();
    let source = GameSourceQualification::from_host(&host).unwrap();
    assert!(translated.admit(root.path(), &providers, &source).is_err());
}

#[test]
fn unqualified_api_cannot_bypass_admission_by_disabling_keyboard_only() {
    let (root, _, mut policy) = setup();
    std::fs::write(
        root.path().join("Assets/Control.cs"),
        "public class Control {}\n",
    )
    .unwrap();
    let receipt = read(&root);
    policy.original_closure_sha256 = receipt.closure_sha256().into();
    let mut host = keyboard_host(root.path(), &receipt);
    host.api.surface = ApiSurface::Unknown;
    let source = GameSourceQualification::from_host(&host).unwrap();
    assert!(
        selection(&receipt, &policy)
            .admit(root.path(), &inventory(), &source)
            .is_err()
    );
}
