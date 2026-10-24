use serde_json::json;
use std::fs;
use worldrunner_compat_content::*;

const GUID: &str = "22222222222222222222222222222222";
fn fixture(class_id: i32, kind: &str, material: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Assets")).unwrap();
    fs::write(root.path().join("Assets/Main.unity"), format!("%YAML 1.1\n--- !u!1 &1\nGameObject:\n  m_Name: Body\n--- !u!4 &2\nTransform:\n  m_GameObject: {{fileID: 1}}\n--- !u!{class_id} &3\n{kind}:\n  m_GameObject: {{fileID: 1}}\n  m_Material: {{fileID: 6200000, guid: {GUID}, type: 2}}\n")).unwrap();
    fs::write(
        root.path().join("Assets/Main.unity.meta"),
        "guid: 11111111111111111111111111111111\n",
    )
    .unwrap();
    fs::write(
        root.path().join("Assets/Bounce.physicsMaterial2D"),
        material,
    )
    .unwrap();
    fs::write(
        root.path().join("Assets/Bounce.physicsMaterial2D.meta"),
        format!("guid: {GUID}\n"),
    )
    .unwrap();
    root
}
fn read(root: &tempfile::TempDir) -> ContentReceipt {
    import_receipt(
        root.path(),
        "Assets/Main.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap()
}
const MATERIAL: &str = "%YAML 1.1\n--- !u!62 &6200000\nPhysicsMaterial2D:\n  m_Name: Bounce\n  m_Bounciness: 1\n  m_Friction: 0\n";
#[test]
fn supported_body_and_collider_physics_materials_remain_admissible() {
    for (class_id, kind) in [
        (50, "Rigidbody2D"),
        (61, "BoxCollider2D"),
        (58, "CircleCollider2D"),
        (60, "PolygonCollider2D"),
    ] {
        let root = fixture(class_id, kind, MATERIAL);
        let receipt = read(&root);
        materialization_plan(receipt.graph(), &AdmissionPolicy::default())
            .unwrap_or_else(|e| panic!("{kind}: {e}"));
        let component = receipt
            .graph()
            .components
            .iter()
            .find(|c| c.kind == kind)
            .unwrap();
        assert_eq!(component.native_fields["material"]["bounciness"], json!(1));
        let admitted = receipt
            .admit(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &TrustedProviderInventory::new(),
            )
            .unwrap_or_else(|e| panic!("{kind}: {e}"));
        let proof = serde_json::to_value(admitted.proof()).unwrap();
        let edge = proof["reference_decisions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["observation"]["reference"]["guid"] == GUID)
            .unwrap();
        assert_eq!(edge["observation"]["resource_kind"], "physics_material_2d");
        assert_eq!(edge["classification"], "required_runtime");
        assert_eq!(
            serde_json::to_value(
                AdmissionProof::from_json(&serde_json::to_vec(admitted.proof()).unwrap()).unwrap()
            )
            .unwrap(),
            proof
        );
        admitted.verify_current(root.path()).unwrap();
    }
}
#[test]
fn physics_material_requires_exact_target_document_class_kind_and_file_id() {
    for material in [
        MATERIAL.replace("!u!62", "!u!21"),
        MATERIAL.replace("PhysicsMaterial2D:", "Material:"),
        MATERIAL.replace("&6200000", "&6200001"),
    ] {
        let receipt = read(&fixture(50, "Rigidbody2D", &material));
        let error = receipt
            .admit(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &TrustedProviderInventory::new(),
            )
            .err()
            .expect("wrong physics target must block");
        assert!(error.proof.reference_decisions.iter().any(|d| {
            d.observation
                .reference
                .as_ref()
                .is_some_and(|r| r.guid == GUID)
                && d.classification == DependencyClassification::Blocked
        }));
    }
}
#[test]
fn render_material_and_wrong_owner_class_cannot_use_physics_materials() {
    for (class_id, kind) in [
        (212, "SpriteRenderer"),
        (50, "BoxCollider2D"),
        (61, "Rigidbody2D"),
    ] {
        let receipt = read(&fixture(class_id, kind, MATERIAL));
        let error = receipt
            .admit(
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &TrustedProviderInventory::new(),
            )
            .err()
            .expect("render or mismatched owner must block");
        let edge = error
            .proof
            .reference_decisions
            .iter()
            .find(|d| {
                d.observation
                    .reference
                    .as_ref()
                    .is_some_and(|r| r.guid == GUID)
            })
            .unwrap();
        assert_eq!(edge.classification, DependencyClassification::Blocked);
        assert_eq!(
            serde_json::to_value(edge.observation.resource_kind).unwrap(),
            "material"
        );
    }
}
#[test]
fn resource_kind_wire_accepts_physics_and_rejects_unknown_variants() {
    let kind: ResourceKind = serde_json::from_value(json!("physics_material_2d")).unwrap();
    assert_eq!(
        serde_json::to_value(kind).unwrap(),
        json!("physics_material_2d")
    );
    for unknown in [
        json!("physics_material"),
        json!("PhysicsMaterial2D"),
        json!({"physics_material_2d":true}),
    ] {
        assert!(serde_json::from_value::<ResourceKind>(unknown).is_err());
    }
}

#[test]
fn tilemap_collider_dependency_is_physics_typed_without_claiming_native_tile_readiness() {
    let receipt = read(&fixture(19_719_996, "TilemapCollider2D", MATERIAL));
    let error = receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .err()
        .unwrap();
    let edge = error
        .proof
        .reference_decisions
        .iter()
        .find(|d| {
            d.observation
                .reference
                .as_ref()
                .is_some_and(|r| r.guid == GUID)
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(edge.observation.resource_kind).unwrap(),
        "physics_material_2d"
    );
    assert_eq!(
        edge.classification,
        DependencyClassification::RequiredRuntime
    );
}
