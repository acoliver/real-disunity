use super::*;

fn native() -> Value {
    serde_json::from_str(include_str!("native_descriptor.json")).unwrap()
}

fn bridge() -> Value {
    json!({"capabilities": [CONSUMER]})
}

fn requirements() -> Value {
    json!({
        "execution_profile": EXECUTION,
        "scheduler_profile": PROFILE,
        "callback_profile": CALLBACK
    })
}

#[test]
fn released_native_descriptor_matches_the_contextual_contract() {
    validate(&native(), &bridge(), &requirements()).unwrap();
}

#[test]
fn contextual_contract_rejects_missing_native_or_managed_capability() {
    let mut provider = native();
    provider["capabilities"] = json!([]);
    assert!(validate(&provider, &bridge(), &requirements()).is_err());
    assert!(validate(&native(), &json!({"capabilities": []}), &requirements()).is_err());
}

#[test]
fn contextual_descriptor_rejects_changed_fields_and_extra_claims() {
    let original = native();
    let fields = original["native_contextual_lifecycle_batch"]
        .as_object()
        .unwrap();
    for name in fields.keys() {
        let mut provider = original.clone();
        provider["native_contextual_lifecycle_batch"][name] = Value::Null;
        assert!(
            validate(&provider, &bridge(), &requirements()).is_err(),
            "{name}"
        );
    }
    let mut provider = original;
    provider["native_contextual_lifecycle_batch"]["full_scheduler"] = json!(true);
    assert!(validate(&provider, &bridge(), &requirements()).is_err());
}

#[test]
fn every_profile_selector_is_required_and_exact() {
    for name in ["execution_profile", "scheduler_profile", "callback_profile"] {
        let mut required = requirements();
        required.as_object_mut().unwrap().remove(name);
        assert!(validate(&native(), &bridge(), &required).is_err());
        required[name] = json!("unsupported");
        assert!(validate(&native(), &bridge(), &required).is_err());
    }
}

#[test]
fn legacy_selection_accepts_absent_requirements_without_batch_capability() {
    for required in [
        Value::Null,
        json!({}),
        json!({"scheduler_profile": "native_readiness_single_v1"}),
    ] {
        assert!(!selection_matches(&required, false).unwrap());
        assert!(selection_matches(&required, true).is_err());
    }
}

#[test]
fn native_and_manifest_contextual_selection_must_match_both_directions() {
    assert!(selection_matches(&requirements(), true).unwrap());
    assert!(selection_matches(&requirements(), false).is_err());
    assert!(selection_matches(&json!({"callback_profile": CALLBACK}), false).is_err());
    let mut required = requirements();
    required.as_object_mut().unwrap().remove("callback_profile");
    assert!(selection_matches(&required, true).is_err());
}

#[test]
fn contextual_terminal_composition_rejects_before_activation() {
    for (field, capability) in [
        ("native_capabilities", super::super::terminal::NATIVE),
        ("bridge_capabilities", super::super::terminal::MANAGED),
    ] {
        let mut required = requirements();
        required[field] = json!([capability]);
        assert!(validate(&native(), &bridge(), &required).is_err());
    }
    let mut required = requirements();
    required["native_capabilities"] = json!([super::super::terminal::NATIVE]);
    required["bridge_capabilities"] = json!([super::super::terminal::MANAGED]);
    assert!(validate(&native(), &bridge(), &required).is_err());
}
