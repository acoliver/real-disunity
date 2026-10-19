use std::sync::Arc;

use worldrunner_render_plan::{
    ContractRegistration, RenderAdapterError, RenderContractError, RenderPayload,
    RenderPayloadAdapter, RenderWorldContractBuilder,
};
use worldrunner_world_model::{ResourceBinding, SemanticComponent, SemanticWorldDocument};
use worldrunner_world_runtime::{
    CapabilityHandler, CapabilitySession, CapabilitySessionError, ComponentHandler, ComponentPlan,
    ComponentRejection, ContributionKind, DocumentHandler, DocumentPlan, DocumentRejection,
    HandlerDescriptor, PreparedPayload, ResourceContribution,
};

const DOCUMENT_SCHEMA: &str = "test:document:v1";
const ROOT_CONTRACT: &str = "test:root-payload:v1";
const COMPONENT_CONTRACT: &str = "test:component-payload:v1";

struct TestAdapter;

impl RenderPayloadAdapter for TestAdapter {
    fn adapt(&self, _payload: &PreparedPayload) -> Result<RenderPayload, RenderAdapterError> {
        Ok(RenderPayload::new(()))
    }
}

struct TestDocumentHandler {
    descriptor: HandlerDescriptor,
}

impl DocumentHandler for TestDocumentHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn plan(&self, document: &SemanticWorldDocument) -> Result<DocumentPlan, DocumentRejection> {
        Ok(DocumentPlan::new(
            self.descriptor.clone(),
            document.schema().clone(),
            Vec::new(),
        ))
    }
}

struct TestComponentHandler {
    descriptor: HandlerDescriptor,
    contract_uri: &'static str,
}

impl ComponentHandler for TestComponentHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn plan(&self, component: &SemanticComponent) -> Result<ComponentPlan, ComponentRejection> {
        Ok(ComponentPlan::new(
            self.descriptor.clone(),
            self.contract_uri,
            component.payload.clone(),
            Vec::new(),
        ))
    }
}
struct TestCapabilityHandler {
    descriptor: HandlerDescriptor,
    contract_uri: &'static str,
}

impl CapabilityHandler for TestCapabilityHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn start_session(
        &self,
        _binding: &ResourceBinding,
    ) -> Result<Box<dyn CapabilitySession>, CapabilitySessionError> {
        Ok(Box::new(TestCapabilitySession {
            descriptor: self.descriptor.clone(),
            contract_uri: self.contract_uri,
        }))
    }
}

struct TestCapabilitySession {
    descriptor: HandlerDescriptor,
    contract_uri: &'static str,
}

impl CapabilitySession for TestCapabilitySession {
    fn write_chunk(&mut self, _chunk: &[u8]) -> Result<(), CapabilitySessionError> {
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ResourceContribution, CapabilitySessionError> {
        Ok(ResourceContribution::new(
            self.descriptor,
            self.contract_uri,
            (),
        ))
    }
}

fn document_handler() -> Arc<dyn DocumentHandler> {
    Arc::new(TestDocumentHandler {
        descriptor: HandlerDescriptor::new("test:document-handler", 1),
    })
}

fn component_handler(id: &str, contract_uri: &'static str) -> Arc<dyn ComponentHandler> {
    Arc::new(TestComponentHandler {
        descriptor: HandlerDescriptor::new(id, 1),
        contract_uri,
    })
}

fn capability_handler(id: &str, contract_uri: &'static str) -> Arc<dyn CapabilityHandler> {
    Arc::new(TestCapabilityHandler {
        descriptor: HandlerDescriptor::new(id, 1),
        contract_uri,
    })
}

fn configured_builder() -> RenderWorldContractBuilder {
    let mut builder = RenderWorldContractBuilder::new();
    builder
        .document_schema(DOCUMENT_SCHEMA, 1)
        .unwrap()
        .document_handler(document_handler())
        .unwrap()
        .root_payload_contract(ROOT_CONTRACT, Arc::new(TestAdapter))
        .unwrap();
    builder
}

#[test]
fn mandatory_document_registration_is_fail_closed() {
    let error = RenderWorldContractBuilder::new().build().unwrap_err();
    assert!(matches!(
        error,
        RenderContractError::MissingRegistration {
            kind: ContractRegistration::DocumentSchema
        }
    ));
}

#[test]
fn malformed_and_duplicate_routes_are_rejected() {
    let mut malformed = configured_builder();
    let error = malformed
        .component_route(
            "not a uri",
            component_handler("test:handler:a", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .err()
        .unwrap();
    assert!(matches!(
        error,
        RenderContractError::InvalidKey {
            kind: ContractRegistration::ComponentRoute,
            ..
        }
    ));

    let mut duplicate = configured_builder();
    duplicate
        .component_route(
            "test:component:a",
            component_handler("test:handler:a", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .unwrap();
    let error = duplicate
        .component_route(
            "test:component:a",
            component_handler("test:handler:b", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .err()
        .unwrap();
    assert!(matches!(
        error,
        RenderContractError::DuplicateRegistration {
            kind: ContractRegistration::ComponentRoute,
            ..
        }
    ));
}

#[test]
fn unresolved_and_orphan_adapters_are_rejected() {
    let mut unresolved = configured_builder();
    unresolved
        .component_route(
            "test:component:a",
            component_handler("test:handler:a", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .unwrap();
    assert!(matches!(
        unresolved.build().unwrap_err(),
        RenderContractError::UnresolvedAdapter {
            route_kind: ContractRegistration::ComponentRoute,
            ..
        }
    ));

    let mut orphan = configured_builder();
    orphan
        .component_adapter(COMPONENT_CONTRACT, Arc::new(TestAdapter))
        .unwrap();
    assert!(matches!(
        orphan.build().unwrap_err(),
        RenderContractError::OrphanAdapter {
            kind: ContributionKind::Component,
            ..
        }
    ));
}

#[test]
fn distinct_routes_may_share_one_payload_contract() {
    let mut builder = configured_builder();
    builder
        .component_adapter(COMPONENT_CONTRACT, Arc::new(TestAdapter))
        .unwrap()
        .component_route(
            "test:component:a",
            component_handler("test:handler:a", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .unwrap()
        .component_route(
            "test:component:b",
            component_handler("test:handler:b", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .unwrap();
    builder.build().unwrap();
}

#[test]
fn malformed_and_duplicate_capability_routes_are_rejected() {
    let mut malformed = configured_builder();
    let error = malformed
        .capability_route(
            "not a uri",
            capability_handler("test:capability-handler:a", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .err()
        .unwrap();
    assert!(matches!(
        error,
        RenderContractError::InvalidKey {
            kind: ContractRegistration::CapabilityRoute,
            ..
        }
    ));

    let mut duplicate = configured_builder();
    duplicate
        .capability_route(
            "test:capability:a",
            capability_handler("test:capability-handler:a", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .unwrap();
    let error = duplicate
        .capability_route(
            "test:capability:a",
            capability_handler("test:capability-handler:b", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .err()
        .unwrap();
    assert!(matches!(
        error,
        RenderContractError::DuplicateRegistration {
            kind: ContractRegistration::CapabilityRoute,
            ..
        }
    ));
}

#[test]
fn capability_adapters_require_exact_route_closure() {
    let mut unresolved = configured_builder();
    unresolved
        .capability_route(
            "test:capability:a",
            capability_handler("test:capability-handler:a", COMPONENT_CONTRACT),
            COMPONENT_CONTRACT,
        )
        .unwrap();
    assert!(matches!(
        unresolved.build().unwrap_err(),
        RenderContractError::UnresolvedAdapter {
            route_kind: ContractRegistration::CapabilityRoute,
            ..
        }
    ));

    let mut orphan = configured_builder();
    orphan
        .resource_adapter(COMPONENT_CONTRACT, Arc::new(TestAdapter))
        .unwrap();
    assert!(matches!(
        orphan.build().unwrap_err(),
        RenderContractError::OrphanAdapter {
            kind: ContributionKind::Resource,
            ..
        }
    ));
}
