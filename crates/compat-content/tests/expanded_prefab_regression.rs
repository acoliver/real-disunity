mod expanded_prefab_support;
use expanded_prefab_support::*;
use serde_json::json;
use worldrunner_compat_content::*;

#[test]
fn expanded_physics_material_override_requires_physics_target_identity() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (OTHER, 2_100_000));
    let receipt = read(&root);
    let live = receipt
        .graph()
        .components
        .iter()
        .find(|c| c.kind == "Rigidbody2D" && !c.source.instance.is_empty())
        .unwrap();
    assert_eq!(
        live.fields["m_Material"]["$ref"],
        json!(format!("{OTHER}:2100000"))
    );
    assert_eq!(live.native_fields["material"]["bounciness"], json!(0.75));
    let proof = proof(&receipt);
    assert_eq!(
        proof.outcome,
        AdmissionOutcome::Blocked,
        "expanded Rigidbody2D m_Material must reject a class21 Material target"
    );
    assert_eq!(
        physics_use(&proof).classification,
        DependencyClassification::Blocked
    );
    assert_eq!(
        physics_use(&proof)
            .observation
            .reference
            .as_ref()
            .unwrap()
            .guid,
        OTHER
    );
    assert!(proof.reference_decisions.iter().any(|d| {
        matches!(
            d.observation.locator.owner,
            OwnerIdentity::AssetDocument { file_id: 100, .. }
        ) && d.observation.resource_kind == ResourceKind::Unknown
            && d.observation
                .reference
                .as_ref()
                .is_some_and(|r| r.guid == OTHER)
    }));
    assert!(proof.reference_decisions.iter().any(|d| matches!(
        d.observation.locator.owner,
        OwnerIdentity::AssetDocument { file_id: 12, .. }
    ) && d.observation.resource_kind
        == ResourceKind::PhysicsMaterial2D
        && d.classification == DependencyClassification::RequiredRuntime));
}
