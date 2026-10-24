use std::fs;
use worldrunner_compat_content::*;

pub const SCENE: &str = "11111111111111111111111111111111";
pub const GOOD: &str = "22222222222222222222222222222222";
pub const PREFAB: &str = "33333333333333333333333333333333";
pub const OTHER: &str = "44444444444444444444444444444444";
pub const PHYSICS: &str = "%YAML 1.1\n--- !u!62 &6200000\nPhysicsMaterial2D:\n  m_Bounciness: 1\n";
pub const RENDER: &str =
    "%YAML 1.1\n--- !u!21 &2100000\nMaterial:\n  m_Name: Render\n  m_Bounciness: 0.75\n";
pub fn write(root: &tempfile::TempDir, path: &str, text: &str) {
    fs::write(root.path().join(path), text).unwrap();
}
pub fn asset(root: &tempfile::TempDir, path: &str, guid: &str, text: &str) {
    write(root, path, text);
    write(root, &format!("{path}.meta"), &format!("guid: {guid}\n"));
}
pub fn fixture(owner: (i32, &str), field: &str, target: (&str, i64)) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Assets")).unwrap();
    let (class, kind) = owner;
    asset(
        &root,
        "Assets/Template.prefab",
        PREFAB,
        &format!(
            "%YAML 1.1\n--- !u!1001 &100100000\nPrefab:\n  m_RootGameObject: {{fileID: 10}}\n--- !u!1 &10\nGameObject:\n  m_Name: Body\n--- !u!4 &11\nTransform:\n  m_GameObject: {{fileID: 10}}\n--- !u!{class} &12\n{kind}:\n  m_GameObject: {{fileID: 10}}\n  m_Material: {{fileID: 6200000, guid: {GOOD}, type: 2}}\n"
        ),
    );
    asset(
        &root,
        "Assets/Main.unity",
        SCENE,
        &instance(PREFAB, field, target),
    );
    asset(&root, "Assets/Good.physicsMaterial2D", GOOD, PHYSICS);
    asset(&root, "Assets/Other.mat", OTHER, RENDER);
    root
}
pub fn instance(prefab: &str, field: &str, target: (&str, i64)) -> String {
    let (guid, file_id) = target;
    format!(
        "%YAML 1.1\n--- !u!1001 &100\nPrefabInstance:\n  m_SourcePrefab: {{fileID: 100100000, guid: {prefab}, type: 3}}\n  m_Modification:\n    m_TransformParent: {{fileID: 0}}\n    m_Modifications:\n    - target: {{fileID: 12, guid: {prefab}, type: 3}}\n      propertyPath: {field}\n      value: \n      objectReference: {{fileID: {file_id}, guid: {guid}, type: 2}}\n"
    )
}
pub fn read(root: &tempfile::TempDir) -> ContentReceipt {
    import_receipt(
        root.path(),
        "Assets/Main.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap()
}
pub fn proof(receipt: &ContentReceipt) -> AdmissionProof {
    match receipt.admit(
        &AdmissionPolicy::default(),
        &DependencyPolicy::default(),
        &TrustedProviderInventory::new(),
    ) {
        Ok(admitted) => admitted.proof().clone(),
        Err(error) => *error.proof,
    }
}
pub fn effective(proof: &AdmissionProof) -> Vec<&ReferenceDecision> {
    proof
        .reference_decisions
        .iter()
        .filter(|d| {
            serde_json::to_value(&d.observation.locator.owner).unwrap()["container"]
                == "expanded_document"
        })
        .collect()
}
pub fn physics_use(proof: &AdmissionProof) -> &ReferenceDecision {
    effective(proof)
        .into_iter()
        .find(|d| d.observation.resource_kind == ResourceKind::PhysicsMaterial2D)
        .expect("effective physics destination must be observed")
}
