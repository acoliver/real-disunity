use std::sync::Arc;

use worldrunner_world_model::{SemanticMap, SemanticWorldDocument};
use worldrunner_world_runtime::{
    ContributionKind, ContributionRequirement, DocumentHandler, DocumentPlan, DocumentRejection,
    HandlerDescriptor, PreparedPayload, RuntimePlanError, RuntimeRegistryBuilder,
};

use super::support::{
    ALPHA_SCHEMA, BYTES_CAPABILITY, MapProvider, alpha_handler, alpha_payload, binding,
    bytes_handler, component, document, prepare, registry_with_document_handler,
};

const COMPONENT_URI: &str = "synthetic:contribution/alpha/unit-alpha";
const RESOURCE_URI: &str = "synthetic:contribution/bytes/binding-alpha";

#[derive(Clone, Debug, Eq, PartialEq)]
struct TypedRootPayload {
    contract: &'static str,
}

struct RootHandler {
    registered: HandlerDescriptor,
    reported: HandlerDescriptor,
    requirements: Vec<ContributionRequirement>,
}

impl RootHandler {
    fn exact() -> Self {
        Self {
            registered: HandlerDescriptor::new("synthetic-handler/root-contract", 17),
            reported: HandlerDescriptor::new("synthetic-handler/root-contract", 17),
            requirements: exact_requirements(),
        }
    }

    fn with_requirements(requirements: Vec<ContributionRequirement>) -> Self {
        Self {
            requirements,
            ..Self::exact()
        }
    }
}

impl DocumentHandler for RootHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.registered
    }

    fn plan(&self, _document: &SemanticWorldDocument) -> Result<DocumentPlan, DocumentRejection> {
        Ok(DocumentPlan::new(
            self.reported.clone(),
            TypedRootPayload { contract: "strict" },
            self.requirements.clone(),
        ))
    }
}

fn component_requirement() -> ContributionRequirement {
    ContributionRequirement::new(ContributionKind::Component, "unit-alpha", COMPONENT_URI)
}

fn resource_requirement() -> ContributionRequirement {
    ContributionRequirement::new(ContributionKind::Resource, "binding-alpha", RESOURCE_URI)
}

fn exact_requirements() -> Vec<ContributionRequirement> {
    vec![component_requirement(), resource_requirement()]
}

fn registry(root: RootHandler) -> worldrunner_world_runtime::RuntimeRegistry {
    registry_with_document_handler(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(BYTES_CAPABILITY, bytes_handler())],
        Arc::new(root),
    )
}

fn alpha_world() -> SemanticWorldDocument {
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

#[test]
fn prepared_payload_downcasts_are_typed_shared_and_immutable() {
    let payload = PreparedPayload::new(TypedRootPayload { contract: "exact" });
    let clone = payload.clone();
    let first = payload
        .downcast_ref::<TypedRootPayload>()
        .expect("correct contract type");
    let second = clone
        .downcast_ref::<TypedRootPayload>()
        .expect("cloned contract type");

    assert_eq!(first.contract, "exact");
    assert!(std::ptr::eq(first, second));
    assert!(payload.downcast_ref::<SemanticMap>().is_none());
}

#[test]
fn exact_root_contract_is_retained_in_prepared_world() {
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let prepared = prepare(&registry(RootHandler::exact()), &alpha_world(), &provider)
        .expect("strict root closure");

    assert_eq!(
        prepared.root_handler(),
        &HandlerDescriptor::new("synthetic-handler/root-contract", 17)
    );
    assert_eq!(
        prepared
            .root_payload()
            .downcast_ref::<TypedRootPayload>()
            .expect("typed root payload")
            .contract,
        "strict"
    );
    assert!(
        prepared
            .root_payload()
            .downcast_ref::<SemanticMap>()
            .is_none()
    );
}

#[test]
fn document_handler_identity_drift_fails_before_open() {
    let mut root = RootHandler::exact();
    root.reported = HandlerDescriptor::new("synthetic-handler/other-root", 17);
    let provider = MapProvider::new().with_source(b"alpha-bytes");

    let error = prepare(&registry(root), &alpha_world(), &provider).unwrap_err();

    assert!(matches!(
        error,
        RuntimePlanError::HandlerIdentityMismatch { .. }
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn unexpected_contribution_fails_exact_root_closure() {
    let root = RootHandler::with_requirements(vec![component_requirement()]);
    let provider = MapProvider::new().with_source(b"alpha-bytes");

    let error = prepare(&registry(root), &alpha_world(), &provider).unwrap_err();

    assert!(matches!(
        error,
        RuntimePlanError::UnexpectedContribution { contribution }
            if contribution == Box::new(resource_requirement())
    ));
}

#[test]
fn missing_contribution_fails_exact_root_closure() {
    let mut requirements = exact_requirements();
    requirements.push(ContributionRequirement::new(
        ContributionKind::Resource,
        "binding-missing",
        "synthetic:contribution/resource/binding-missing",
    ));
    let provider = MapProvider::new().with_source(b"alpha-bytes");

    let error = prepare(
        &registry(RootHandler::with_requirements(requirements)),
        &alpha_world(),
        &provider,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        RuntimePlanError::MissingContribution { .. }
    ));
}

#[test]
fn duplicate_and_misordered_root_requirements_fail_before_open() {
    let duplicate = vec![component_requirement(), component_requirement()];
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let error = prepare(
        &registry(RootHandler::with_requirements(duplicate)),
        &alpha_world(),
        &provider,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::DuplicateContributionRequirement { .. }
    ));
    assert_eq!(provider.open_count(), 0);

    let reversed = vec![resource_requirement(), component_requirement()];
    let error = prepare(
        &registry(RootHandler::with_requirements(reversed)),
        &alpha_world(),
        &provider,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::InvalidContributionRequirementOrder { .. }
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn registry_without_document_handler_fails_closed() {
    let mut builder = RuntimeRegistryBuilder::new();
    builder
        .document_schema("synthetic:world/runtime", 1)
        .expect("document schema");

    assert!(matches!(
        builder.build(),
        Err(worldrunner_world_runtime::RuntimeRegistryError::MissingDocumentHandler)
    ));
}
