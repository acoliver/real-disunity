//! Exact authored-profile contract lookups are deliberately branchy: each group rejects or
//! accepts a specific mutation, so the per-case arms cannot be factored down.
#![allow(clippy::cognitive_complexity)]

use serde_json::{Value, json};
use worldrunner_world_runtime::{
    AuthoredComponentPayload, AuthoredRootPayload, AuthoredRuntimeProfile, PreparedSemanticWorld,
    RUNTIME_PROFILE_CAPABILITY_URI, RuntimeProfileError, RuntimeProfileLimits,
    VerifiedResourcePayload,
};

use super::support::{
    ALPHA_SCHEMA, BYTES_CAPABILITY, DOC_TAG, DOC_VERSION, MapProvider, binding, binding_ref,
    component, document, map, prepare, text,
};

const BYTES_ROLE: &str = "synthetic:role/bytes";
const BYTES_ALIAS_ROLE: &str = "synthetic:role/bytes-alias";
const PROFILE_ROLE: &str = "synthetic:role/profile";
const COMPONENT_CONTRACT: &str = "synthetic:prepared/component";
const RESOURCE_CONTRACT: &str = "synthetic:prepared/resource";
const ROOT_CONTRACT: &str = "synthetic:prepared/root";

fn limits() -> RuntimeProfileLimits {
    RuntimeProfileLimits::new(4, 4, 4).expect("positive limits")
}

fn profile_document() -> Value {
    json!({
        "schema": "real-disunity.runtime-profile.v1",
        "schema_version": 1,
        "profile_id": "synthetic-runtime-profile",
        "world_schema": { "tag": DOC_TAG, "version": DOC_VERSION },
        "root_handler": {
            "handler_id": "generic-root",
            "revision": 1,
            "payload_contract_uri": ROOT_CONTRACT
        },
        "component_handlers": [{
            "source_schema_uri": ALPHA_SCHEMA,
            "handler_id": "generic-component",
            "revision": 2,
            "payload_contract_uri": COMPONENT_CONTRACT,
            "roles": [
                {
                    "role_uri": BYTES_ROLE,
                    "capability_uri": BYTES_CAPABILITY,
                    "element_uri": null
                },
                {
                    "role_uri": PROFILE_ROLE,
                    "capability_uri": RUNTIME_PROFILE_CAPABILITY_URI,
                    "element_uri": null
                }
            ]
        }],
        "capability_handlers": [
            {
                "capability_uri": BYTES_CAPABILITY,
                "element_uri": null,
                "handler_id": "generic-bytes",
                "revision": 3,
                "payload_contract_uri": RESOURCE_CONTRACT
            },
            {
                "capability_uri": RUNTIME_PROFILE_CAPABILITY_URI,
                "element_uri": null,
                "handler_id": "generic-profile",
                "revision": 4,
                "payload_contract_uri": "synthetic:prepared/profile"
            }
        ]
    })
}

fn assert_prepared(prepared: &PreparedSemanticWorld) {
    assert_eq!(prepared.component_contributions().len(), 1);
    assert_eq!(prepared.resource_contributions().len(), 2);
    assert_eq!(
        prepared.handler_revisions(),
        &[
            ("generic-bytes".to_owned(), 3),
            ("generic-component".to_owned(), 2),
            ("generic-profile".to_owned(), 4),
            ("generic-root".to_owned(), 1),
        ]
        .into_iter()
        .collect()
    );
    let root = prepared
        .root_payload()
        .downcast_ref::<AuthoredRootPayload>()
        .unwrap();
    assert_eq!(root.profile_id, "synthetic-runtime-profile");
    assert_eq!(root.payload_contract_uri, ROOT_CONTRACT);
    assert_eq!(root.component_count, 1);
    assert_eq!(root.resource_count, 2);
    let component = prepared.component_contributions()[0]
        .payload
        .downcast_ref::<AuthoredComponentPayload>()
        .unwrap();
    assert_eq!(component.roles[BYTES_ROLE], "resource-a");
    assert_eq!(component.roles[PROFILE_ROLE], "runtime-profile");
    assert_eq!(component.source, text("preserved-source-semantics"));
    for contribution in prepared.resource_contributions() {
        let payload = contribution
            .payload
            .downcast_ref::<VerifiedResourcePayload>()
            .unwrap();
        assert!(payload.byte_length > 0);
    }
}

#[test]
fn profile_builds_generic_handlers_and_prepares_exact_closure() {
    let profile_bytes = serde_json::to_vec(&profile_document()).unwrap();
    let resource_bytes = b"arbitrary-resource";
    let profile = AuthoredRuntimeProfile::from_json(&profile_bytes, limits()).unwrap();
    let registry = profile.build_registry().unwrap();
    let document = document(
        vec![component(
            "component-a",
            ALPHA_SCHEMA,
            map(&[
                (
                    "roles",
                    worldrunner_world_model::SemanticValue::Map(map(&[
                        (BYTES_ROLE, binding_ref("resource-a")),
                        (PROFILE_ROLE, binding_ref("runtime-profile")),
                    ])),
                ),
                ("source", text("preserved-source-semantics")),
            ]),
        )],
        vec![
            binding("resource-a", BYTES_CAPABILITY, resource_bytes, None),
            binding(
                "runtime-profile",
                RUNTIME_PROFILE_CAPABILITY_URI,
                &profile_bytes,
                None,
            ),
        ],
    );
    let provider = MapProvider::new()
        .with_source(resource_bytes)
        .with_source(&profile_bytes);

    let prepared = prepare(&registry, &document, &provider).unwrap();
    assert_eq!(provider.open_count(), 2);
    assert_prepared(&prepared);
}

#[test]
fn validated_contracts_are_available_through_read_only_exact_lookups() {
    let bytes = serde_json::to_vec(&profile_document()).unwrap();
    let profile = AuthoredRuntimeProfile::from_json(&bytes, limits()).unwrap();

    assert_eq!(profile.root_handler().handler_id(), "generic-root");
    assert_eq!(profile.root_handler().revision(), 1);
    assert_eq!(profile.root_handler().payload_contract_uri(), ROOT_CONTRACT);

    let component = profile.component_handler(ALPHA_SCHEMA).unwrap();
    assert_eq!(component.source_schema_uri(), ALPHA_SCHEMA);
    assert_eq!(component.handler_id(), "generic-component");
    assert_eq!(component.revision(), 2);
    assert_eq!(component.payload_contract_uri(), COMPONENT_CONTRACT);
    assert_eq!(component.roles().len(), 2);
    assert_eq!(component.roles()[0].role_uri(), BYTES_ROLE);
    assert_eq!(component.roles()[0].capability_uri(), BYTES_CAPABILITY);
    assert_eq!(component.roles()[0].element_uri(), None);
    assert_eq!(
        profile.component_handlers(),
        std::slice::from_ref(component)
    );
    assert!(
        profile
            .component_handler("synthetic:missing-component:v1")
            .is_none()
    );

    let capability = profile.capability_handler(BYTES_CAPABILITY).unwrap();
    assert_eq!(capability.capability_uri(), BYTES_CAPABILITY);
    assert_eq!(capability.element_uri(), None);
    assert_eq!(capability.handler_id(), "generic-bytes");
    assert_eq!(capability.revision(), 3);
    assert_eq!(capability.payload_contract_uri(), RESOURCE_CONTRACT);
    assert_eq!(profile.capability_handlers().len(), 2);
    assert!(
        profile
            .capability_handler("synthetic:missing-capability:v1")
            .is_none()
    );
}

fn profile_with_second_component_schema() -> Value {
    let mut profile = profile_document();
    let mut handler = profile["component_handlers"][0].clone();
    handler["source_schema_uri"] = json!(super::support::BETA_SCHEMA);
    handler["handler_id"] = json!("generic-component-beta");
    profile["component_handlers"]
        .as_array_mut()
        .unwrap()
        .push(handler);
    profile
}

fn authored_component(id: &str, schema: &str) -> worldrunner_world_model::SemanticComponent {
    component(
        id,
        schema,
        map(&[
            (
                "roles",
                worldrunner_world_model::SemanticValue::Map(map(&[
                    (BYTES_ROLE, binding_ref("resource-a")),
                    (PROFILE_ROLE, binding_ref("runtime-profile")),
                ])),
            ),
            ("source", text("preserved-source-semantics")),
        ]),
    )
}

#[test]
fn authored_component_schema_allows_zero_one_or_multiple_instances() {
    for instance_count in 0..=2 {
        let profile_bytes = serde_json::to_vec(&profile_with_second_component_schema()).unwrap();
        let profile = AuthoredRuntimeProfile::from_json(&profile_bytes, limits()).unwrap();
        let registry = profile.build_registry().unwrap();
        let mut components = vec![authored_component("carrier", super::support::BETA_SCHEMA)];
        components.extend(
            (0..instance_count)
                .map(|index| authored_component(&format!("repeatable-{index}"), ALPHA_SCHEMA)),
        );
        let resource_bytes = b"arbitrary-resource";
        let world = document(
            components,
            vec![
                binding("resource-a", BYTES_CAPABILITY, resource_bytes, None),
                binding(
                    "runtime-profile",
                    RUNTIME_PROFILE_CAPABILITY_URI,
                    &profile_bytes,
                    None,
                ),
            ],
        );
        let provider = MapProvider::new()
            .with_source(resource_bytes)
            .with_source(&profile_bytes);

        let prepared = prepare(&registry, &world, &provider).unwrap();
        assert_eq!(prepared.component_contributions().len(), instance_count + 1);
        assert_eq!(prepared.resource_contributions().len(), 2);
        assert_eq!(provider.open_count(), 2);
    }
}

#[test]
fn profile_fails_closed_on_schema_order_limits_and_unhandled_role_capability() {
    let mut wrong_schema = profile_document();
    wrong_schema["schema_version"] = json!(2);
    assert!(
        AuthoredRuntimeProfile::from_json(&serde_json::to_vec(&wrong_schema).unwrap(), limits())
            .is_err()
    );

    let mut unordered = profile_document();
    unordered["component_handlers"][0]["roles"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert!(
        AuthoredRuntimeProfile::from_json(&serde_json::to_vec(&unordered).unwrap(), limits())
            .is_err()
    );

    assert!(
        AuthoredRuntimeProfile::from_json(
            &serde_json::to_vec(&profile_document()).unwrap(),
            RuntimeProfileLimits::new(1, 1, 1).unwrap(),
        )
        .is_err()
    );

    let mut missing_capability = profile_document();
    missing_capability["capability_handlers"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(
        AuthoredRuntimeProfile::from_json(
            &serde_json::to_vec(&missing_capability).unwrap(),
            limits(),
        )
        .is_err()
    );
}
#[test]
fn profile_capability_must_route_a_bare_binding() {
    let mut profile = profile_document();
    profile["capability_handlers"][1]["element_uri"] = json!("synthetic:element/profile");
    assert!(matches!(
        AuthoredRuntimeProfile::from_json(&serde_json::to_vec(&profile).unwrap(), limits()),
        Err(RuntimeProfileError::ProfileCapabilityElementMismatch { .. })
    ));
}

#[test]
fn component_roles_may_alias_one_exact_binding() {
    let mut profile_value = profile_document();
    let mut alias_role = profile_value["component_handlers"][0]["roles"][0].clone();
    alias_role["role_uri"] = json!(BYTES_ALIAS_ROLE);
    profile_value["component_handlers"][0]["roles"]
        .as_array_mut()
        .unwrap()
        .insert(1, alias_role);
    let profile_bytes = serde_json::to_vec(&profile_value).unwrap();
    let profile = AuthoredRuntimeProfile::from_json(&profile_bytes, limits()).unwrap();
    let registry = profile.build_registry().unwrap();
    let resource_bytes = b"aliased-resource";
    let world = document(
        vec![component(
            "component-a",
            ALPHA_SCHEMA,
            map(&[
                (
                    "roles",
                    worldrunner_world_model::SemanticValue::Map(map(&[
                        (BYTES_ROLE, binding_ref("resource-a")),
                        (BYTES_ALIAS_ROLE, binding_ref("resource-a")),
                        (PROFILE_ROLE, binding_ref("runtime-profile")),
                    ])),
                ),
                ("source", text("source")),
            ]),
        )],
        vec![
            binding("resource-a", BYTES_CAPABILITY, resource_bytes, None),
            binding(
                "runtime-profile",
                RUNTIME_PROFILE_CAPABILITY_URI,
                &profile_bytes,
                None,
            ),
        ],
    );
    let provider = MapProvider::new()
        .with_source(resource_bytes)
        .with_source(&profile_bytes);

    let prepared = prepare(&registry, &world, &provider).unwrap();
    assert_eq!(prepared.component_contributions().len(), 1);
    let component = prepared.component_contributions()[0]
        .payload
        .downcast_ref::<AuthoredComponentPayload>()
        .unwrap();
    assert_eq!(component.roles[BYTES_ROLE], "resource-a");
    assert_eq!(component.roles[BYTES_ALIAS_ROLE], "resource-a");
    assert_eq!(component.roles[PROFILE_ROLE], "runtime-profile");
}

#[test]
fn component_handler_rejects_payload_and_role_drift_before_opening_resources() {
    let profile_bytes = serde_json::to_vec(&profile_document()).unwrap();
    let profile = AuthoredRuntimeProfile::from_json(&profile_bytes, limits()).unwrap();
    let registry = profile.build_registry().unwrap();
    let document = document(
        vec![component(
            "component-a",
            ALPHA_SCHEMA,
            map(&[
                (
                    "roles",
                    worldrunner_world_model::SemanticValue::Map(map(&[
                        (BYTES_ROLE, binding_ref("resource-a")),
                        (PROFILE_ROLE, binding_ref("resource-a")),
                    ])),
                ),
                ("source", text("source")),
            ]),
        )],
        vec![binding("resource-a", BYTES_CAPABILITY, b"resource", None)],
    );
    let provider = MapProvider::new().with_source(b"resource");
    assert!(prepare(&registry, &document, &provider).is_err());
    assert_eq!(provider.open_count(), 0);
}
