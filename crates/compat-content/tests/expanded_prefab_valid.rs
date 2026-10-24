mod expanded_prefab_support;
use expanded_prefab_support::*;
use serde_json::json;
use worldrunner_compat_content::*;

#[test]
fn physics_to_distinct_physics_override_emits_only_selected_physics_values() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (OTHER, 6_200_000));
    write(
        &root,
        "Assets/Other.mat",
        &format!("{RENDER}--- !u!62 &6200000\nPhysicsMaterial2D:\n  m_Bounciness: 0.25\n"),
    );
    let receipt = read(&root);
    let proof = proof(&receipt);
    assert_eq!(proof.outcome, AdmissionOutcome::AdmittedOriginal);
    let edge = physics_use(&proof);
    assert_eq!(
        edge.classification,
        DependencyClassification::RequiredRuntime
    );
    assert_eq!(edge.observation.reference.as_ref().unwrap().guid, OTHER);
    let admitted = receipt
        .admit(
            &AdmissionPolicy::default(),
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let mut requests = Vec::new();
    let mut next = 100_u64;
    admitted.materialization_plan().execute(|request| {
        next += 2;
        let response = match request["op"].as_str().unwrap() {
            "object.create" => json!({"handle": next, "components": [{"handle": next + 1, "kind": "Transform"}]}),
            "component.add" => json!({"handle": next}),
            _ => serde_json::Value::Null,
        };
        requests.push(request);
        Ok::<_, String>(response)
    }).unwrap();
    assert_eq!(requests.len(), 12);
    assert!(requests.iter().any(
        |r| r["op"] == "component.set" && r["fields"]["material"]["bounciness"] == json!(0.25)
    ));
    assert!(
        !requests
            .iter()
            .any(|r| r["fields"]["material"]["bounciness"] == json!(0.75))
    );
}

#[test]
fn valid_material_array_override_remains_admissible_without_a_shader_provider() {
    let root = fixture(
        (50, "Rigidbody2D"),
        "m_Materials.Array.data[0]",
        (OTHER, 2_100_000),
    );
    let receipt = read(&root);
    let proof = proof(&receipt);
    assert_eq!(proof.outcome, AdmissionOutcome::AdmittedOriginal);
    assert_eq!(
        physics_use(&proof).classification,
        DependencyClassification::RequiredRuntime
    );
    let edge = effective(&proof)
        .into_iter()
        .find(|d| d.observation.resource_kind == ResourceKind::Material)
        .unwrap();
    assert_eq!(
        edge.classification,
        DependencyClassification::RequiredRuntime
    );
}

#[test]
fn one_invalid_instance_blocks_even_when_another_effective_instance_is_valid() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (GOOD, 6_200_000));
    let second = instance(PREFAB, "m_Material", (OTHER, 2_100_000))
        .replace("%YAML 1.1\n", "")
        .replace("&100\n", "&101\n");
    write(
        &root,
        "Assets/Main.unity",
        &(instance(PREFAB, "m_Material", (GOOD, 6_200_000)) + &second),
    );
    let receipt = read(&root);
    let proof = proof(&receipt);
    assert_eq!(proof.outcome, AdmissionOutcome::Blocked);
    let edges: Vec<_> = effective(&proof)
        .into_iter()
        .filter(|d| d.observation.resource_kind == ResourceKind::PhysicsMaterial2D)
        .collect();
    assert_eq!(edges.len(), 2);
    assert_eq!(
        physics_use(&proof).classification,
        DependencyClassification::RequiredRuntime
    );
    assert_eq!(
        edges
            .iter()
            .filter(|d| d.classification == DependencyClassification::Blocked)
            .count(),
        1
    );
    let wrong = edges
        .iter()
        .find(|d| d.classification == DependencyClassification::Blocked)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&wrong.observation.locator.owner).unwrap()["instance"],
        format!("{SCENE}:101")
    );
}

#[test]
fn effective_observations_preserve_legitimate_aliases_and_component_omission_ids() {
    let root = fixture((50, "Rigidbody2D"), "m_Material", (GOOD, 6_200_000));
    let scene = instance(PREFAB, "m_Material", (GOOD, 6_200_000));
    write(
        &root,
        "Assets/Main.unity",
        &format!(
            "{scene}--- !u!4 &200 stripped\nTransform:\n  m_CorrespondingSourceObject: {{fileID: 11, guid: {PREFAB}, type: 3}}\n  m_PrefabInstance: {{fileID: 100}}\n"
        ),
    );
    let receipt = read(&root);
    assert_eq!(proof(&receipt).outcome, AdmissionOutcome::AdmittedOriginal);
    assert_eq!(
        physics_use(&proof(&receipt)).classification,
        DependencyClassification::RequiredRuntime
    );
    let live = format!("{PREFAB}:12@{SCENE}:100");
    let components = AdmissionPolicy {
        ignored_components: [(live.clone(), "explicit live body omission".into())].into(),
        ..AdmissionPolicy::default()
    };
    let admitted = receipt
        .admit(
            &components,
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        admitted.proof().component_omissions,
        components.ignored_components
    );
    assert_eq!(
        receipt.graph().aliases[&format!("{SCENE}:200")],
        format!("{PREFAB}:11@{SCENE}:100")
    );
    admitted
        .verify_inputs(
            root.path(),
            &components,
            &DependencyPolicy::default(),
            &TrustedProviderInventory::new(),
        )
        .unwrap();
    assert!(
        !admitted.proof().reference_decisions.iter().any(|d| d
            .observation
            .locator
            .source_path
            .contains('@'))
    );
}
