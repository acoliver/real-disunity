use super::*;

#[test]
fn shader_replacement_rejects_nested_and_mixed_values_at_admission() {
    let (root, receipt, policy) = setup();
    for replacement in [
        json!([{"nested":{"$ref":"11111111111111111111111111111111:2",
            "guid":"11111111111111111111111111111111","file_id":2}}]),
        json!([null, 7, {"not_a_shader_reference":true}]),
        json!({"nested":{"$builtin":"unqualified"}}),
    ] {
        let mut changed = policy.clone();
        if let SelectionOperation::ReplaceField {
            replacement: value, ..
        } = &mut changed.operations[0]
        {
            *value = replacement;
        }
        assert!(
            selection(&receipt, &changed)
                .admit(root.path(), &inventory(), &qualification(&receipt))
                .is_err()
        );
    }
}

#[test]
fn shader_replacement_retains_explicit_null_and_empty_list_support() {
    let (root, receipt, policy) = setup();
    for replacement in [json!([]), json!([null])] {
        let mut changed = policy.clone();
        if let SelectionOperation::ReplaceField {
            replacement: value, ..
        } = &mut changed.operations[0]
        {
            *value = replacement;
        }
        let providers = inventory();
        let source = qualification(&receipt);
        selection(&receipt, &changed)
            .admit(root.path(), &providers, &source)
            .unwrap()
            .verify_inputs(root.path(), &changed, &providers, &source)
            .unwrap();
    }
}
