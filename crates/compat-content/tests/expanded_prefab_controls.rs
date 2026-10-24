mod expanded_prefab_support;
use expanded_prefab_support::*;
use serde_json::json;
use std::collections::BTreeSet;
use worldrunner_compat_content::*;

#[test]
fn expanded_use_retains_source_provenance_and_unique_real_instance_binding() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (GOOD, 6_200_000));
    let receipt = read(&root);
    let admitted = receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let proof = admitted.proof();
    let edge = physics_use(proof);
    let owner = serde_json::to_value(&edge.observation.locator.owner).unwrap();
    assert_eq!(owner["instance"], format!("{SCENE}:100"));
    assert_eq!(owner["file_id"], 12);
    assert_eq!(owner["document_index"], 3);
    assert_eq!(owner["class_id"], 50);
    assert_eq!(
        edge.observation.source_sha256,
        receipt.graph().source_hashes["Assets/Template.prefab"]
    );
    assert_eq!(
        edge.classification,
        DependencyClassification::RequiredRuntime
    );
    assert!(proof.reference_decisions.iter().any(|d| matches!(
        d.observation.locator.owner,
        OwnerIdentity::AssetDocument { file_id: 12, .. }
    ) && d.observation.resource_kind
        == ResourceKind::PhysicsMaterial2D));
    assert_eq!(
        proof
            .reference_decisions
            .iter()
            .map(|d| &d.observation.locator)
            .collect::<BTreeSet<_>>()
            .len(),
        proof.reference_decisions.len()
    );
}

#[test]
fn effective_uses_are_sealed_and_requalified_against_current_receipt() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (GOOD, 6_200_000));
    let receipt = read(&root);
    let admitted = receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let proof = admitted.proof();
    admitted
        .verify_inputs(
            root.path(),
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .unwrap();
    AdmissionProof::from_json(&serde_json::to_vec(proof).unwrap()).unwrap();
    let mut changed = proof.clone();
    changed.reference_decisions.retain(|d| {
        serde_json::to_value(&d.observation.locator.owner).unwrap()["container"]
            != "expanded_document"
    });
    assert!(
        receipt
            .verify_admission(
                &changed,
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &TrustedProviderInventory::new()
            )
            .is_err()
    );
    assert!(AdmissionProof::from_json(&serde_json::to_vec(&changed).unwrap()).is_err());
}

#[test]
fn effective_physics_target_checks_exact_class_kind_and_file_in_mixed_asset() {
    for material in [
        RENDER.to_string(),
        format!("{RENDER}--- !u!62 &6200000\nPhysicsMaterial2D:\n  m_Bounciness: 1\n"),
        RENDER.replace("!u!21", "!u!62"),
        RENDER.replace("Material:", "PhysicsMaterial2D:"),
    ] {
        let root = fixture((50, "Rigidbody2D"), "m_Material", (OTHER, 2_100_000));
        write(&root, "Assets/Other.mat", &material);
        let receipt = read(&root);
        let proof = proof(&receipt);
        assert_eq!(proof.outcome, AdmissionOutcome::Blocked);
        let edge = physics_use(&proof);
        assert_eq!(edge.classification, DependencyClassification::Blocked);
        assert_eq!(
            edge.observation.reference.as_ref().unwrap().file_id,
            2_100_000
        );
        assert_eq!(edge.observation.reference.as_ref().unwrap().guid, OTHER);
        assert!(proof.reference_decisions.iter().any(|d| matches!(
            d.observation.locator.owner,
            OwnerIdentity::AssetDocument { file_id: 12, .. }
        ) && d.classification
            == DependencyClassification::RequiredRuntime));
    }
}

#[test]
fn physics_override_from_null_and_missing_field_is_admissible() {
    for replacement in ["m_Material: {fileID: 0}", "unused: 1"] {
        let root = fixture((50, "Rigidbody2D"), "m_Material", (GOOD, 6_200_000));
        let path = root.path().join("Assets/Template.prefab");
        let text = std::fs::read_to_string(&path).unwrap().replace(
            &format!("m_Material: {{fileID: 6200000, guid: {GOOD}, type: 2}}"),
            replacement,
        );
        std::fs::write(path, text).unwrap();
        let receipt = read(&root);
        let proof = proof(&receipt);
        assert_eq!(
            proof.outcome,
            AdmissionOutcome::AdmittedOriginal,
            "{:?}",
            proof.blockers
        );
        assert_eq!(
            physics_use(&proof).classification,
            DependencyClassification::RequiredRuntime
        );
    }
}

#[test]
fn effective_arrays_keep_destination_kind_and_full_index_path() {
    let root = fixture(
        (50, "Rigidbody2D"),
        "custom.Array.data[1].m_Material",
        (OTHER, 2_100_000),
    );
    let receipt = read(&root);
    let proof = proof(&receipt);
    assert_eq!(proof.outcome, AdmissionOutcome::Blocked);
    let edge = physics_use(&proof);
    assert_eq!(
        edge.observation.locator.field,
        vec![
            FieldSegment::Key("custom".into()),
            FieldSegment::Index(1),
            FieldSegment::Key("m_Material".into())
        ]
    );
    assert_eq!(edge.classification, DependencyClassification::Blocked);
}

#[test]
fn every_nested_prefab_use_has_distinct_destination_and_retained_provenance() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (OTHER, 2_100_000));
    let nested = "55555555555555555555555555555555";
    asset(
        &root,
        "Assets/Nested.prefab",
        nested,
        &instance(PREFAB, "m_Material", (OTHER, 2_100_000)),
    );
    write(
        &root,
        "Assets/Main.unity",
        &instance(nested, "unused", (GOOD, 6_200_000))
            .replace("fileID: 12, guid:", "fileID: 100, guid:"),
    );
    let receipt = read(&root);
    let proof = proof(&receipt);
    assert_eq!(proof.outcome, AdmissionOutcome::Blocked);
    let edges: Vec<_> = effective(&proof)
        .into_iter()
        .filter(|d| d.observation.resource_kind == ResourceKind::PhysicsMaterial2D)
        .collect();
    assert_eq!(edges.len(), 2);
    assert!(
        edges
            .iter()
            .all(|d| d.classification == DependencyClassification::Blocked)
    );
    assert!(edges.iter().any(
        |d| serde_json::to_value(&d.observation.locator.owner).unwrap()["instance"]
            == format!("{nested}:100@{SCENE}:100")
    ));
}

#[test]
fn effective_material_array_and_shader_slots_do_not_acquire_physics_semantics() {
    for field in ["m_Materials.Array.data[0]", "m_Shader"] {
        let root = fixture((50, "Rigidbody2D"), field, (GOOD, 6_200_000));
        let receipt = read(&root);
        let proof = proof(&receipt);
        let expected = if field == "m_Shader" {
            ResourceKind::Shader
        } else {
            ResourceKind::Material
        };
        let edge = effective(&proof)
            .into_iter()
            .find(|d| d.observation.resource_kind == expected)
            .unwrap();
        assert_eq!(edge.classification, DependencyClassification::Blocked);
        assert_eq!(proof.outcome, AdmissionOutcome::Blocked);
    }
}

#[test]
fn wrong_effective_owner_class_cannot_claim_physics_semantics() {
    for owner in [
        (61, "Rigidbody2D"),
        (50, "BoxCollider2D"),
        (212, "SpriteRenderer"),
    ] {
        let root = fixture(owner, "m_Material", (GOOD, 6_200_000));
        let receipt = read(&root);
        let proof = proof(&receipt);
        let edge = effective(&proof)
            .into_iter()
            .find(|d| d.observation.locator.field == [FieldSegment::Key("m_Material".into())])
            .unwrap();
        assert_eq!(edge.observation.resource_kind, ResourceKind::Material);
        assert_eq!(edge.classification, DependencyClassification::Blocked);
    }
}

#[test]
fn expanded_render_target_requires_material_document_identity() {
    let root = fixture(
        (50, "Rigidbody2D"),
        "m_Materials.Array.data[0]",
        (OTHER, 6_200_000),
    );
    write(
        &root,
        "Assets/Other.mat",
        &format!("{RENDER}--- !u!62 &6200000\nPhysicsMaterial2D:\n  m_Bounciness: 1\n"),
    );
    let receipt = read(&root);
    let proof = proof(&receipt);
    let edge = effective(&proof)
        .into_iter()
        .find(|d| d.observation.resource_kind == ResourceKind::Material)
        .unwrap();
    assert_eq!(edge.classification, DependencyClassification::Blocked);
    assert_eq!(edge.observation.raw_reference["file_id"], json!(6_200_000));
}

#[test]
fn same_id_effective_instance_field_and_target_tampering_rejects() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (GOOD, 6_200_000));
    let receipt = read(&root);
    let proof = proof(&receipt);
    assert_eq!(
        physics_use(&proof).classification,
        DependencyClassification::RequiredRuntime
    );
    let index = proof
        .reference_decisions
        .iter()
        .position(|d| {
            matches!(
                d.observation.locator.owner,
                OwnerIdentity::ExpandedDocument { .. }
            ) && d.observation.resource_kind == ResourceKind::PhysicsMaterial2D
        })
        .unwrap();
    for (path, replacement) in [
        ("/locator/owner/instance", json!(format!("{SCENE}:101"))),
        ("/locator/owner/class_id", json!(61)),
        ("/locator/field/0/value", json!("m_Shader")),
        ("/reference/file_id", json!(6_200_001)),
        ("/raw_reference/$ref", json!(format!("{OTHER}:2100000"))),
    ] {
        let mut changed = serde_json::to_value(&proof).unwrap();
        *changed["reference_decisions"][index]["observation"]
            .pointer_mut(path)
            .unwrap() = replacement;
        let bytes = serde_json::to_vec(&changed).unwrap();
        assert!(AdmissionProof::from_json(&bytes).is_err());
        let changed: AdmissionProof = serde_json::from_slice(&bytes).unwrap();
        assert!(
            receipt
                .verify_admission(
                    &changed,
                    &AdmissionPolicy::default(),
                    &DependencyPolicy::default(),
                    &TrustedProviderInventory::new()
                )
                .is_err()
        );
    }
}

#[test]
fn changed_valid_override_needs_fresh_qualification_and_new_effective_binding() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (GOOD, 6_200_000));
    write(&root, "Assets/Other.mat", PHYSICS);
    let receipt = read(&root);
    let admitted = receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    write(
        &root,
        "Assets/Main.unity",
        &instance(PREFAB, "m_Material", (OTHER, 6_200_000)),
    );
    assert!(
        admitted
            .verify_inputs(
                root.path(),
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &TrustedProviderInventory::new()
            )
            .is_err()
    );
    let current = read(&root);
    let proof = proof(&current);
    assert_eq!(proof.outcome, AdmissionOutcome::AdmittedOriginal);
    assert_ne!(proof.admission_sha256, admitted.proof().admission_sha256);
    assert_eq!(
        physics_use(&proof)
            .observation
            .reference
            .as_ref()
            .unwrap()
            .guid,
        OTHER
    );
    assert!(
        current
            .verify_admission(
                admitted.proof(),
                &AdmissionPolicy::default(),
                &DependencyPolicy::default(),
                &TrustedProviderInventory::new()
            )
            .is_err()
    );
}
