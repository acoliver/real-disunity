//! Identifier-independent runtime routing regressions.

use super::support::*;

use std::sync::Arc;

use worldrunner_world_runtime::{
    HandlerKind, RuntimePlanError, RuntimeRegistryBuilder, RuntimeRegistryError,
};

#[test]
fn renaming_ids_leaves_uri_routing_unchanged() {
    let registry = standard_registry();
    let first_provider = MapProvider::new().with_source(b"alpha-bytes");
    let first = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );
    let second_provider = MapProvider::new().with_source(b"alpha-bytes");
    let second = document(
        vec![component(
            "unit-zeta",
            ALPHA_SCHEMA,
            alpha_payload("binding-zeta"),
        )],
        vec![binding(
            "binding-zeta",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let first_prepared =
        prepare(&registry, &first, &first_provider).expect("first naming prepares");
    let second_prepared =
        prepare(&registry, &second, &second_provider).expect("renamed identifiers prepare");

    assert_eq!(
        first_prepared.handler_revisions(),
        second_prepared.handler_revisions()
    );
    let first_handlers: Vec<_> = first_prepared
        .component_contributions()
        .iter()
        .map(|contribution| contribution.handler.handler_id.clone())
        .collect();
    let second_handlers: Vec<_> = second_prepared
        .component_contributions()
        .iter()
        .map(|contribution| contribution.handler.handler_id.clone())
        .collect();
    assert_eq!(first_handlers, second_handlers);
    assert_eq!(
        second_prepared.component_contributions()[0].claimant,
        "unit-zeta"
    );
    assert_eq!(
        second_prepared.resource_contributions()[0].claimant,
        "binding-zeta"
    );
}

#[test]
fn malformed_registry_routes_are_rejected() {
    let mut builder = RuntimeRegistryBuilder::new();
    assert!(matches!(
        builder.document_schema("not a uri", DOC_VERSION),
        Err(RuntimeRegistryError::InvalidRoutingKey {
            handler_kind: HandlerKind::DocumentSchema,
            key,
            ..
        }) if key == "not a uri"
    ));

    builder
        .document_schema(DOC_TAG, DOC_VERSION)
        .expect("valid document route");
    assert!(matches!(
        builder.component_handler("1bad:component", Arc::new(alpha_handler())),
        Err(RuntimeRegistryError::InvalidRoutingKey {
            handler_kind: HandlerKind::Component,
            ..
        })
    ));
    assert!(matches!(
        builder.capability_handler("bad capability", Arc::new(bytes_handler())),
        Err(RuntimeRegistryError::InvalidRoutingKey {
            handler_kind: HandlerKind::Capability,
            ..
        })
    ));
}

#[test]
fn malformed_component_payload_contract_uri_fails_before_open() {
    let registry = registry_of(
        vec![(
            ALPHA_SCHEMA,
            alpha_handler().with_contribution_override("not a uri"),
        )],
        vec![(BYTES_CAPABILITY, bytes_handler())],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let world = alpha_world();

    let error = prepare(&registry, &world, &provider).unwrap_err();

    assert!(matches!(
        error,
        RuntimePlanError::InvalidHandlerOutput {
            claimant,
            field: "payload_contract_uri",
            ..
        } if claimant == "unit-alpha"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn malformed_requirement_identifier_and_uris_fail_before_open() {
    assert_invalid_requirement(
        alpha_handler().with_binding_override("bad\u{0}binding"),
        "binding_requirement.binding_id",
    );
    assert_invalid_requirement(
        alpha_handler().with_resource_contract("not a uri", None, "synthetic:contribution/alpha"),
        "binding_requirement.capability_uri",
    );
    assert_invalid_requirement(
        alpha_handler().with_resource_contract(
            BYTES_CAPABILITY,
            Some("bad element"),
            "synthetic:contribution/alpha",
        ),
        "binding_requirement.element_uri",
    );
}

#[test]
fn malformed_resource_payload_contract_uri_is_rejected() {
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(
            BYTES_CAPABILITY,
            bytes_handler().with_contribution_override("not a uri"),
        )],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let world = alpha_world();

    let error = prepare(&registry, &world, &provider).unwrap_err();

    assert!(matches!(
        error,
        RuntimePlanError::InvalidHandlerOutput {
            claimant,
            field: "payload_contract_uri",
            ..
        } if claimant == "binding-alpha"
    ));
    assert_eq!(provider.open_count(), 0);
}

fn assert_invalid_requirement(handler: SyntheticComponentHandler, expected_field: &'static str) {
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, handler)],
        vec![(BYTES_CAPABILITY, bytes_handler())],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let error = prepare(&registry, &alpha_world(), &provider).unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::InvalidHandlerOutput { field, .. } if field == expected_field
    ));
    assert_eq!(provider.open_count(), 0);
}

fn alpha_world() -> worldrunner_world_model::SemanticWorldDocument {
    document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    )
}
