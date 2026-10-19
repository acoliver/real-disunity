//! Fail-closed negotiation contract tests with arbitrary synthetic names.
//!
//! Every schema, capability, handler identity, and identifier is invented for
//! these tests alone; no dataset, corpus, or production naming appears.

use std::sync::Arc;

use worldrunner_world_model::SemanticMap;
use worldrunner_world_runtime::{
    HandlerDescriptor, HandlerKind, RuntimePlanError, RuntimeRegistryBuilder, RuntimeRegistryError,
};

use super::support::*;

#[test]
fn known_schema_components_and_bindings_prepare_completely() {
    let registry = standard_registry();
    let provider = MapProvider::new()
        .with_source(b"alpha-bytes")
        .with_source(b"beta-bytes");
    let document = document(
        vec![
            component("unit-alpha", ALPHA_SCHEMA, alpha_payload("binding-alpha")),
            component("unit-beta", BETA_SCHEMA, beta_payload("binding-beta")),
        ],
        vec![
            binding("binding-alpha", BYTES_CAPABILITY, b"alpha-bytes", None),
            binding(
                "binding-beta",
                ELEMENT_CAPABILITY,
                b"beta-bytes",
                Some(ELEMENT_LEAF),
            ),
        ],
    );

    let prepared = prepare(&registry, &document, &provider).expect("fully negotiated world");

    assert_eq!(prepared.schema().tag, DOC_TAG);
    assert_eq!(prepared.schema().version, DOC_VERSION);
    assert_eq!(*prepared.source(), *document.source());

    assert_prepared_contributions(&prepared);
    assert_eq!(provider.open_count(), 2);
    assert_eq!(
        provider.opened(),
        vec![
            content_address(b"alpha-bytes"),
            content_address(b"beta-bytes")
        ]
    );
}

fn assert_prepared_contributions(prepared: &worldrunner_world_runtime::PreparedSemanticWorld) {
    let component_uris: Vec<&str> = prepared
        .component_contributions()
        .iter()
        .map(|contribution| contribution.payload_contract_uri.as_str())
        .collect();
    let mut sorted_components = component_uris.clone();
    sorted_components.sort_unstable();
    assert_eq!(component_uris, sorted_components);
    let resource_uris: Vec<&str> = prepared
        .resource_contributions()
        .iter()
        .map(|contribution| contribution.payload_contract_uri.as_str())
        .collect();
    let mut sorted_resources = resource_uris.clone();
    sorted_resources.sort_unstable();
    assert_eq!(resource_uris, sorted_resources);

    let revisions = prepared.handler_revisions();
    assert_eq!(revisions.len(), 5);
    assert_eq!(revisions.get("synthetic-handler/document"), Some(&13));
    assert_eq!(revisions.get("synthetic-handler/alpha"), Some(&7));
    assert_eq!(revisions.get("synthetic-handler/beta"), Some(&3));
    assert_eq!(revisions.get("synthetic-handler/bytes"), Some(&11));
    assert_eq!(revisions.get("synthetic-handler/element"), Some(&5));

    let alpha = prepared
        .component_contributions()
        .iter()
        .find(|contribution| contribution.claimant == "unit-alpha")
        .expect("alpha component contribution");
    assert_eq!(
        alpha
            .payload
            .downcast_ref::<SemanticMap>()
            .unwrap()
            .get("origin"),
        Some(&text("synthetic-handler/alpha"))
    );
    let resource = prepared
        .resource_contributions()
        .iter()
        .find(|contribution| contribution.claimant == "binding-alpha")
        .expect("alpha resource contribution");
    assert_eq!(
        resource
            .payload
            .downcast_ref::<SemanticMap>()
            .unwrap()
            .get("bytes"),
        Some(&unsigned(11))
    );
}

#[test]
fn unknown_top_level_schema_fails_before_any_open() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document_with_tag(
        "synthetic:world/other",
        DOC_VERSION,
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

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::UnsupportedDocumentSchema { expected, observed }
        if expected.tag == DOC_TAG
            && expected.version == DOC_VERSION
            && observed.tag == "synthetic:world/other"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn unknown_component_schema_fails_before_any_open() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            "synthetic:component/absent",
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::UnsupportedComponentSchema { component_id, schema_uri }
        if component_id == "unit-alpha" && schema_uri == "synthetic:component/absent"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn unknown_capability_fails_before_any_open() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            "synthetic:capability/absent",
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::UnsupportedCapability { binding_id, capability_uri }
        if binding_id == "binding-alpha"
            && capability_uri == "synthetic:capability/absent"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn unsupported_unreferenced_binding_fails_before_any_open() {
    let registry = standard_registry();
    let provider = MapProvider::new()
        .with_source(b"alpha-bytes")
        .with_source(b"orphan-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![
            binding("binding-alpha", BYTES_CAPABILITY, b"alpha-bytes", None),
            binding(
                "binding-orphan",
                "synthetic:capability/absent",
                b"orphan-bytes",
                None,
            ),
        ],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::UnsupportedCapability { binding_id, .. }
        if binding_id == "binding-orphan"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn absent_handler_creates_no_fallback() {
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(BYTES_CAPABILITY, bytes_handler())],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
        vec![component(
            "unit-beta",
            BETA_SCHEMA,
            beta_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::UnsupportedComponentSchema { component_id, schema_uri }
        if component_id == "unit-beta" && schema_uri == BETA_SCHEMA
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn duplicate_registrations_fail_in_the_builder() {
    let mut builder = RuntimeRegistryBuilder::new();
    builder
        .document_schema(DOC_TAG, DOC_VERSION)
        .expect("first schema registration");
    assert!(matches!(
        builder.document_schema(DOC_TAG, DOC_VERSION),
        Err(RuntimeRegistryError::DuplicateHandler {
            handler_kind: HandlerKind::DocumentSchema,
            ..
        })
    ));
    builder
        .component_handler(ALPHA_SCHEMA, Arc::new(alpha_handler()))
        .expect("first component registration");
    assert!(matches!(
        builder.component_handler(ALPHA_SCHEMA, Arc::new(alpha_handler())),
        Err(RuntimeRegistryError::DuplicateHandler {
            handler_kind: HandlerKind::Component,
            key,
        }) if key == ALPHA_SCHEMA
    ));
    builder
        .capability_handler(BYTES_CAPABILITY, Arc::new(bytes_handler()))
        .expect("first capability registration");
    assert!(matches!(
        builder.capability_handler(BYTES_CAPABILITY, Arc::new(bytes_handler())),
        Err(RuntimeRegistryError::DuplicateHandler {
            handler_kind: HandlerKind::Capability,
            key,
        }) if key == BYTES_CAPABILITY
    ));
}

#[test]
fn builder_requires_exactly_one_document_schema() {
    let error = RuntimeRegistryBuilder::new()
        .build()
        .expect_err("schema-less registry cannot exist");
    assert!(matches!(error, RuntimeRegistryError::MissingDocumentSchema));
}

#[test]
fn unusable_handler_identity_is_refused_at_registration() {
    let mut builder = RuntimeRegistryBuilder::new();
    builder
        .document_schema(DOC_TAG, DOC_VERSION)
        .expect("schema registration");
    let mut corrupted = alpha_handler();
    corrupted.descriptor = HandlerDescriptor::new("synthetic\u{0}handler", 7);
    assert!(matches!(
        builder.component_handler(ALPHA_SCHEMA, Arc::new(corrupted)),
        Err(RuntimeRegistryError::InvalidHandlerDescriptor { handler_id })
        if handler_id == "synthetic\u{0}handler"
    ));
}

#[test]
fn wrong_required_capability_fails_before_any_open() {
    let registry = registry_of(
        vec![(
            ALPHA_SCHEMA,
            SyntheticComponentHandler::new(
                "synthetic-handler/alpha",
                7,
                ALPHA_SCHEMA,
                "strict-alpha",
            )
            .with_resource_contract(
                "synthetic:capability/wrong",
                None,
                "synthetic:contribution/alpha",
            ),
        )],
        vec![(BYTES_CAPABILITY, bytes_handler())],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
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

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::BindingCapabilityMismatch { component_id, binding_id, required, observed }
        if component_id == "unit-alpha"
            && binding_id == "binding-alpha"
            && required == "synthetic:capability/wrong"
            && observed == BYTES_CAPABILITY
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn wrong_required_element_fails_before_any_open() {
    let registry = registry_of(
        vec![(
            ALPHA_SCHEMA,
            SyntheticComponentHandler::new(
                "synthetic-handler/alpha",
                7,
                ALPHA_SCHEMA,
                "strict-alpha",
            )
            .with_resource_contract(
                BYTES_CAPABILITY,
                Some(ELEMENT_LEAF),
                "synthetic:contribution/alpha",
            ),
        )],
        vec![(BYTES_CAPABILITY, bytes_handler())],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
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

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::BindingElementMismatch { component_id, binding_id, required, observed }
        if component_id == "unit-alpha"
            && binding_id == "binding-alpha"
            && required.as_deref() == Some(ELEMENT_LEAF)
            && observed.is_none()
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn mismatched_element_spelling_fails_before_any_open() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_source(b"beta-bytes");
    let document = document(
        vec![component(
            "unit-beta",
            BETA_SCHEMA,
            beta_payload("binding-beta"),
        )],
        vec![binding(
            "binding-beta",
            ELEMENT_CAPABILITY,
            b"beta-bytes",
            Some("synthetic:element/other"),
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::BindingElementMismatch { required, observed, .. }
        if required.as_deref() == Some(ELEMENT_LEAF)
            && observed.as_deref() == Some("synthetic:element/other")
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn unconsumed_binding_fails_before_any_open() {
    let registry = standard_registry();
    let provider = MapProvider::new()
        .with_source(b"alpha-bytes")
        .with_source(b"orphan-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![
            binding("binding-alpha", BYTES_CAPABILITY, b"alpha-bytes", None),
            binding("binding-orphan", BYTES_CAPABILITY, b"orphan-bytes", None),
        ],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::UnconsumedBinding { binding_id, capability_uri }
        if binding_id == "binding-orphan" && capability_uri == BYTES_CAPABILITY
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn duplicate_requirement_fails_before_any_open() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_multi_payload(&["binding-alpha", "binding-alpha"]),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::DuplicateRequirement { component_id, requirement }
        if component_id == "unit-alpha"
            && requirement.binding_id == "binding-alpha"
            && requirement.capability_uri == BYTES_CAPABILITY
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn out_of_order_requirements_fail_before_any_open() {
    let registry = standard_registry();
    let provider = MapProvider::new()
        .with_source(b"alpha-bytes")
        .with_source(b"beta-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_multi_payload(&["binding-beta", "binding-alpha"]),
        )],
        vec![
            binding("binding-alpha", BYTES_CAPABILITY, b"alpha-bytes", None),
            binding("binding-beta", BYTES_CAPABILITY, b"beta-bytes", None),
        ],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::InvalidRequirementOrder { component_id, previous, next }
        if component_id == "unit-alpha"
            && previous.binding_id == "binding-beta"
            && next.binding_id == "binding-alpha"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn unresolved_requirement_fails_before_any_open() {
    let registry = registry_of(
        vec![(
            ALPHA_SCHEMA,
            alpha_handler().with_binding_override("binding-missing"),
        )],
        vec![(BYTES_CAPABILITY, bytes_handler())],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
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

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::UnresolvedBindingRequirement { component_id, binding_id }
        if component_id == "unit-alpha" && binding_id == "binding-missing"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn repeated_payload_contract_uri_across_distinct_claimants_is_valid() {
    let registry = registry_of(
        vec![(
            ALPHA_SCHEMA,
            alpha_handler().with_contribution_override("synthetic:contribution/collision"),
        )],
        vec![(
            BYTES_CAPABILITY,
            bytes_handler().with_contribution_override("synthetic:contribution/collision"),
        )],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
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

    let prepared = prepare(&registry, &document, &provider).unwrap();

    assert_eq!(prepared.component_contributions().len(), 1);
    assert_eq!(prepared.resource_contributions().len(), 1);
    assert_eq!(
        prepared.component_contributions()[0].payload_contract_uri,
        prepared.resource_contributions()[0].payload_contract_uri
    );
}

#[test]
fn component_handler_rejection_preserves_field_expected_observed() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let payload = map(&[
        ("mode", text("loose")),
        ("binding", binding_ref("binding-alpha")),
    ]);
    let document = document(
        vec![component("unit-alpha", ALPHA_SCHEMA, payload)],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::ComponentRejected { component_id, handler_id, rejection, .. }
        if component_id == "unit-alpha"
            && handler_id == "synthetic-handler/alpha"
            && rejection.field == "mode"
            && rejection.expected == "strict-alpha"
            && rejection.observed == "Text(\"loose\")"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn missing_binding_field_is_rejected_with_absent_observed() {
    let registry = standard_registry();
    let provider = MapProvider::new();
    let payload = map(&[("mode", text("strict-alpha"))]);
    let document = document(
        vec![component("unit-alpha", ALPHA_SCHEMA, payload)],
        Vec::new(),
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::ComponentRejected { rejection, .. }
        if rejection.field == "binding"
            && rejection.expected == "binding reference"
            && rejection.observed == "absent"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn component_identity_drift_is_refused() {
    let registry = registry_of(
        vec![(
            ALPHA_SCHEMA,
            alpha_handler().with_reported_identity("synthetic-handler/impostor", 1),
        )],
        vec![(BYTES_CAPABILITY, bytes_handler())],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
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

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::HandlerIdentityMismatch { claimant, registered, reported }
        if claimant == "unit-alpha"
            && registered.handler_id == "synthetic-handler/alpha"
            && reported.handler_id == "synthetic-handler/impostor"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn resource_identity_drift_is_refused() {
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(
            BYTES_CAPABILITY,
            bytes_handler().with_reported_identity("synthetic-handler/impostor", 2),
        )],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
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

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::HandlerIdentityMismatch { claimant, .. }
        if claimant == "binding-alpha"
    ));
}
