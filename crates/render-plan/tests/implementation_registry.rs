//! Activation tests for the descriptor-keyed render implementation registry.
//!
//! Every fixture is synthetic: factories are registered under exact generic
//! URIs and the authored document comes from the shared synthetic contract.
//! Failure tests replace exactly one factory or registration so every
//! rejection is attributable to one invariant.

mod common;

use std::sync::{Arc, Mutex};

use common::{contract_bytes, valid_contract};
use serde_json::json;
use worldrunner_render_plan::{
    BindingElementPolicy, CapabilityImplementation, CapabilityImplementationFactory,
    CapabilityRouteDeclaration, ComponentImplementation, ComponentImplementationFactory,
    ComponentRouteDeclaration, ContractDocument, ContractDocumentError, DocumentImplementation,
    DocumentImplementationFactory, DocumentRouteDeclaration, DocumentSceneInput,
    RenderAdapterError, RenderFactoryError, RenderImplementationError,
    RenderImplementationRegistry, RenderImplementationRegistryBuilder, RenderPayload,
    RenderPayloadAdapter, RenderRouteKind, RenderScene, RouteIdentity, RouteSceneFactory,
    RouteSceneInput, SceneCompositionFactory, SceneContribution, activate_render_world_contract,
};
use worldrunner_world_model::{ResourceBinding, SemanticComponent, SemanticWorldDocument};
use worldrunner_world_runtime::{
    CapabilityHandler, CapabilitySession, CapabilitySessionError, ComponentHandler, ComponentPlan,
    ComponentRejection, DocumentHandler, DocumentPlan, DocumentRejection, HandlerDescriptor,
    PreparedPayload, ResourceContribution,
};

const DOCUMENT_FACTORY: &str = "synthetic:factory/document-handler";
const MESH_FACTORY: &str = "synthetic:factory/mesh-handler";
const WATER_FACTORY: &str = "synthetic:factory/water-handler";
const HEIGHT_FACTORY: &str = "synthetic:factory/height-handler";
const DRIFTED_HANDLER_ID: &str = "synthetic-drifted-handler";
const DRIFTED_HANDLER_REVISION: u64 = 99;
const REFUSAL: &str = "synthetic factory refusal";

struct ProbeAdapter;

impl RenderPayloadAdapter for ProbeAdapter {
    fn adapt(&self, _payload: &PreparedPayload) -> Result<RenderPayload, RenderAdapterError> {
        Ok(RenderPayload::new(()))
    }
}

struct UnusedRouteSceneFactory;

impl RouteSceneFactory for UnusedRouteSceneFactory {
    fn compose_contribution(
        &self,
        _input: RouteSceneInput<'_>,
    ) -> Result<SceneContribution, RenderFactoryError> {
        Err(RenderFactoryError::new(
            "scene composition is not exercised",
        ))
    }
}

struct UnusedSceneComposer;

impl SceneCompositionFactory for UnusedSceneComposer {
    fn compose_scene(
        &self,
        _input: DocumentSceneInput<'_>,
    ) -> Result<RenderScene, RenderFactoryError> {
        Err(RenderFactoryError::new(
            "scene composition is not exercised",
        ))
    }
}

struct StubDocumentHandler {
    descriptor: HandlerDescriptor,
}

impl DocumentHandler for StubDocumentHandler {
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

struct StubComponentHandler {
    descriptor: HandlerDescriptor,
    contract_uri: String,
}

impl ComponentHandler for StubComponentHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn plan(&self, component: &SemanticComponent) -> Result<ComponentPlan, ComponentRejection> {
        Ok(ComponentPlan::new(
            self.descriptor.clone(),
            &self.contract_uri,
            component.payload.clone(),
            Vec::new(),
        ))
    }
}

struct StubCapabilityHandler {
    descriptor: HandlerDescriptor,
    contract_uri: String,
}

impl CapabilityHandler for StubCapabilityHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn start_session(
        &self,
        _binding: &ResourceBinding,
    ) -> Result<Box<dyn CapabilitySession>, CapabilitySessionError> {
        Ok(Box::new(StubCapabilitySession {
            descriptor: self.descriptor.clone(),
            contract_uri: self.contract_uri.clone(),
        }))
    }
}

struct StubCapabilitySession {
    descriptor: HandlerDescriptor,
    contract_uri: String,
}

impl CapabilitySession for StubCapabilitySession {
    fn write_chunk(&mut self, _chunk: &[u8]) -> Result<(), CapabilitySessionError> {
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ResourceContribution, CapabilitySessionError> {
        Ok(ResourceContribution::new(
            self.descriptor,
            &self.contract_uri,
            (),
        ))
    }
}

/// One declaration as observed verbatim by a factory.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedRoute {
    route_key: String,
    role_uri: String,
    payload_contract_uri: String,
    handler_id: String,
    handler_revision: u64,
    cardinality: Option<(u64, u64)>,
    element_policy: Option<BindingElementPolicy>,
}

/// What one stub factory produces for the exact declaration it receives.
#[derive(Clone, Copy)]
enum FactoryBehaviour {
    Faithful,
    Drifted,
    Refuse,
    MissingScene,
}

fn produced_descriptor(behaviour: FactoryBehaviour, identity: &RouteIdentity) -> HandlerDescriptor {
    match behaviour {
        FactoryBehaviour::Faithful | FactoryBehaviour::Refuse | FactoryBehaviour::MissingScene => {
            HandlerDescriptor::new(identity.handler_id(), identity.handler_revision())
        }
        FactoryBehaviour::Drifted => {
            HandlerDescriptor::new(DRIFTED_HANDLER_ID, DRIFTED_HANDLER_REVISION)
        }
    }
}

fn refuse_if_configured(behaviour: FactoryBehaviour) -> Result<(), RenderFactoryError> {
    match behaviour {
        FactoryBehaviour::Refuse => Err(RenderFactoryError::new(REFUSAL)),
        FactoryBehaviour::Faithful | FactoryBehaviour::Drifted | FactoryBehaviour::MissingScene => {
            Ok(())
        }
    }
}

fn record_capture(
    captured: &Mutex<Vec<CapturedRoute>>,
    route_key: &str,
    identity: &RouteIdentity,
    cardinality: Option<(u64, u64)>,
    element_policy: Option<BindingElementPolicy>,
) {
    captured
        .lock()
        .expect("capture lock is uncontended")
        .push(CapturedRoute {
            route_key: route_key.to_owned(),
            role_uri: identity.role_uri().to_owned(),
            payload_contract_uri: identity.payload_contract_uri().to_owned(),
            handler_id: identity.handler_id().to_owned(),
            handler_revision: identity.handler_revision(),
            cardinality,
            element_policy,
        });
}

fn captures_of(captured: &Mutex<Vec<CapturedRoute>>) -> Vec<CapturedRoute> {
    captured
        .lock()
        .expect("capture lock is uncontended")
        .clone()
}

struct StubDocumentFactory {
    behaviour: FactoryBehaviour,
    captured: Mutex<Vec<CapturedRoute>>,
}

impl StubDocumentFactory {
    fn new(behaviour: FactoryBehaviour) -> Self {
        Self {
            behaviour,
            captured: Mutex::new(Vec::new()),
        }
    }

    fn captures(&self) -> Vec<CapturedRoute> {
        captures_of(&self.captured)
    }
}

impl DocumentImplementationFactory for StubDocumentFactory {
    fn build_implementation(
        &self,
        declaration: &DocumentRouteDeclaration,
    ) -> Result<DocumentImplementation, RenderFactoryError> {
        let identity = declaration.identity();
        record_capture(
            &self.captured,
            declaration.schema_uri(),
            identity,
            None,
            None,
        );
        refuse_if_configured(self.behaviour)?;
        let implementation = DocumentImplementation::new(
            Arc::new(StubDocumentHandler {
                descriptor: produced_descriptor(self.behaviour, identity),
            }),
            Arc::new(ProbeAdapter),
        );
        if matches!(self.behaviour, FactoryBehaviour::MissingScene) {
            return Ok(implementation);
        }
        Ok(implementation.with_scene_composer(Arc::new(UnusedSceneComposer)))
    }
}

struct StubComponentFactory {
    behaviour: FactoryBehaviour,
    captured: Mutex<Vec<CapturedRoute>>,
}

impl StubComponentFactory {
    fn new(behaviour: FactoryBehaviour) -> Self {
        Self {
            behaviour,
            captured: Mutex::new(Vec::new()),
        }
    }

    fn captures(&self) -> Vec<CapturedRoute> {
        captures_of(&self.captured)
    }
}

impl ComponentImplementationFactory for StubComponentFactory {
    fn build_implementation(
        &self,
        declaration: &ComponentRouteDeclaration,
    ) -> Result<ComponentImplementation, RenderFactoryError> {
        let identity = declaration.identity();
        record_capture(
            &self.captured,
            declaration.schema_uri(),
            identity,
            Some((
                declaration.cardinality().minimum(),
                declaration.cardinality().maximum(),
            )),
            None,
        );
        refuse_if_configured(self.behaviour)?;
        let implementation = ComponentImplementation::new(
            Arc::new(StubComponentHandler {
                descriptor: produced_descriptor(self.behaviour, identity),
                contract_uri: identity.payload_contract_uri().to_owned(),
            }),
            Arc::new(ProbeAdapter),
        );
        if matches!(self.behaviour, FactoryBehaviour::MissingScene) {
            return Ok(implementation);
        }
        Ok(implementation.with_scene_factory(Arc::new(UnusedRouteSceneFactory)))
    }
}

struct StubCapabilityFactory {
    behaviour: FactoryBehaviour,
    captured: Mutex<Vec<CapturedRoute>>,
}

impl StubCapabilityFactory {
    fn new(behaviour: FactoryBehaviour) -> Self {
        Self {
            behaviour,
            captured: Mutex::new(Vec::new()),
        }
    }

    fn captures(&self) -> Vec<CapturedRoute> {
        captures_of(&self.captured)
    }
}

impl CapabilityImplementationFactory for StubCapabilityFactory {
    fn build_implementation(
        &self,
        declaration: &CapabilityRouteDeclaration,
    ) -> Result<CapabilityImplementation, RenderFactoryError> {
        let identity = declaration.identity();
        record_capture(
            &self.captured,
            declaration.capability_uri(),
            identity,
            Some((
                declaration.cardinality().minimum(),
                declaration.cardinality().maximum(),
            )),
            Some(declaration.element_policy().clone()),
        );
        refuse_if_configured(self.behaviour)?;
        let implementation = CapabilityImplementation::new(
            Arc::new(StubCapabilityHandler {
                descriptor: produced_descriptor(self.behaviour, identity),
                contract_uri: identity.payload_contract_uri().to_owned(),
            }),
            Arc::new(ProbeAdapter),
        );
        if matches!(self.behaviour, FactoryBehaviour::MissingScene) {
            return Ok(implementation);
        }
        Ok(implementation.with_scene_factory(Arc::new(UnusedRouteSceneFactory)))
    }
}

/// Component factory that always returns one shared adapter instance.
struct SharedAdapterComponentFactory {
    adapter: Arc<dyn RenderPayloadAdapter>,
}

impl ComponentImplementationFactory for SharedAdapterComponentFactory {
    fn build_implementation(
        &self,
        declaration: &ComponentRouteDeclaration,
    ) -> Result<ComponentImplementation, RenderFactoryError> {
        let identity = declaration.identity();
        Ok(ComponentImplementation::new(
            Arc::new(StubComponentHandler {
                descriptor: HandlerDescriptor::new(
                    identity.handler_id(),
                    identity.handler_revision(),
                ),
                contract_uri: identity.payload_contract_uri().to_owned(),
            }),
            Arc::clone(&self.adapter),
        )
        .with_scene_factory(Arc::new(UnusedRouteSceneFactory)))
    }
}

fn fixture_document() -> ContractDocument {
    ContractDocument::from_json_bytes(&contract_bytes(&valid_contract()))
        .expect("synthetic fixture decodes")
}

fn document_with_shared_component_payload() -> ContractDocument {
    let mut contract = valid_contract();
    contract["routes"]["components"][1]["payload_contract_uri"] =
        json!("synthetic:payload/mesh-payload");
    ContractDocument::from_json_bytes(&contract_bytes(&contract))
        .expect("shared payload fixture decodes")
}

/// Registers faithful factories of the selected route kinds only.
fn partial_registry(
    document: bool,
    component: bool,
    capability: bool,
) -> RenderImplementationRegistry {
    let mut builder = RenderImplementationRegistryBuilder::new();
    if document {
        builder
            .document_factory(
                DOCUMENT_FACTORY,
                Arc::new(StubDocumentFactory::new(FactoryBehaviour::Faithful)),
            )
            .expect("document factory registers");
    }
    if component {
        for uri in [MESH_FACTORY, WATER_FACTORY] {
            builder
                .component_factory(
                    uri,
                    Arc::new(StubComponentFactory::new(FactoryBehaviour::Faithful)),
                )
                .expect("component factory registers");
        }
    }
    if capability {
        for uri in [MESH_FACTORY, HEIGHT_FACTORY] {
            builder
                .capability_factory(
                    uri,
                    Arc::new(StubCapabilityFactory::new(FactoryBehaviour::Faithful)),
                )
                .expect("capability factory registers");
        }
    }
    builder.build()
}

/// Registers every kind with per-factory behaviours; faithful behaviour
/// reproduces the faithful registry.
fn custom_registry(
    document: FactoryBehaviour,
    mesh_component: FactoryBehaviour,
    water_component: FactoryBehaviour,
    mesh_capability: FactoryBehaviour,
    height_capability: FactoryBehaviour,
) -> RenderImplementationRegistry {
    let mut builder = RenderImplementationRegistryBuilder::new();
    builder
        .document_factory(
            DOCUMENT_FACTORY,
            Arc::new(StubDocumentFactory::new(document)),
        )
        .expect("document factory registers")
        .component_factory(
            MESH_FACTORY,
            Arc::new(StubComponentFactory::new(mesh_component)),
        )
        .expect("mesh component factory registers")
        .component_factory(
            WATER_FACTORY,
            Arc::new(StubComponentFactory::new(water_component)),
        )
        .expect("water component factory registers")
        .capability_factory(
            MESH_FACTORY,
            Arc::new(StubCapabilityFactory::new(mesh_capability)),
        )
        .expect("mesh capability factory registers")
        .capability_factory(
            HEIGHT_FACTORY,
            Arc::new(StubCapabilityFactory::new(height_capability)),
        )
        .expect("height capability factory registers");
    builder.build()
}

fn faithful_registry() -> RenderImplementationRegistry {
    partial_registry(true, true, true)
}

fn registry_with_missing_scene(kind: RenderRouteKind) -> RenderImplementationRegistry {
    let behaviour = |route_kind| {
        if kind == route_kind {
            FactoryBehaviour::MissingScene
        } else {
            FactoryBehaviour::Faithful
        }
    };
    custom_registry(
        behaviour(RenderRouteKind::Document),
        behaviour(RenderRouteKind::Component),
        FactoryBehaviour::Faithful,
        behaviour(RenderRouteKind::Capability),
        FactoryBehaviour::Faithful,
    )
}

#[test]
fn exact_declarations_activate_without_interpreting_authored_roles() {
    let document = fixture_document();
    let document_factory = Arc::new(StubDocumentFactory::new(FactoryBehaviour::Faithful));
    let mesh_component = Arc::new(StubComponentFactory::new(FactoryBehaviour::Faithful));
    let mesh_capability = Arc::new(StubCapabilityFactory::new(FactoryBehaviour::Faithful));
    let mut builder = RenderImplementationRegistryBuilder::new();
    builder
        .document_factory(DOCUMENT_FACTORY, document_factory.clone())
        .expect("document factory registers")
        .component_factory(MESH_FACTORY, mesh_component.clone())
        .expect("mesh component factory registers")
        .component_factory(
            WATER_FACTORY,
            Arc::new(StubComponentFactory::new(FactoryBehaviour::Faithful)),
        )
        .expect("water component factory registers")
        .capability_factory(MESH_FACTORY, mesh_capability.clone())
        .expect("mesh capability factory registers")
        .capability_factory(
            HEIGHT_FACTORY,
            Arc::new(StubCapabilityFactory::new(FactoryBehaviour::Faithful)),
        )
        .expect("height capability factory registers");

    let contract = activate_render_world_contract(&document, &builder.build())
        .expect("all exact declarations activate");

    assert_eq!(
        contract.runtime_registry().document_schema().tag,
        "synthetic:schema/document-alpha"
    );
    assert_eq!(
        contract.root_payload_contract_uri(),
        "synthetic:payload/document-payload"
    );
    assert_eq!(
        document_factory.captures()[0].role_uri,
        "synthetic:role/document"
    );
    assert_eq!(mesh_component.captures()[0].cardinality, Some((0, 16)));
    assert_eq!(
        mesh_capability.captures()[0].element_policy,
        Some(BindingElementPolicy::Bare {})
    );
}

#[test]
fn every_present_route_kind_requires_its_exact_factory() {
    let document = fixture_document();
    for (registry, expected_kind) in [
        (
            partial_registry(false, true, true),
            RenderRouteKind::Document,
        ),
        (
            partial_registry(true, false, true),
            RenderRouteKind::Component,
        ),
        (
            partial_registry(true, true, false),
            RenderRouteKind::Capability,
        ),
    ] {
        assert!(matches!(
            activate_render_world_contract(&document, &registry),
            Err(RenderImplementationError::UnknownFactory { route_kind, .. })
                if route_kind == expected_kind
        ));
    }
}

#[test]
fn every_route_kind_requires_its_activation_bound_scene_factory() {
    let cases = [
        (
            registry_with_missing_scene(RenderRouteKind::Document),
            RenderRouteKind::Document,
            "synthetic:schema/document-alpha",
            DOCUMENT_FACTORY,
        ),
        (
            registry_with_missing_scene(RenderRouteKind::Component),
            RenderRouteKind::Component,
            "synthetic:schema/component-mesh",
            MESH_FACTORY,
        ),
        (
            registry_with_missing_scene(RenderRouteKind::Capability),
            RenderRouteKind::Capability,
            "synthetic:capability/mesh-artifact",
            MESH_FACTORY,
        ),
    ];
    for (registry, expected_kind, expected_key, expected_factory) in cases {
        assert!(matches!(
            activate_render_world_contract(&fixture_document(), &registry),
            Err(RenderImplementationError::MissingSceneFactory {
                route_kind,
                route_key,
                handler_factory_uri,
            }) if route_kind == expected_kind
                && route_key == expected_key
                && handler_factory_uri == expected_factory
        ));
    }
}

#[test]
fn registrations_are_exact_duplicate_free_and_kind_scoped() {
    let mut builder = RenderImplementationRegistryBuilder::new();
    builder
        .component_factory(
            MESH_FACTORY,
            Arc::new(StubComponentFactory::new(FactoryBehaviour::Faithful)),
        )
        .expect("first component factory registers");
    assert!(matches!(
        builder.component_factory(
            MESH_FACTORY,
            Arc::new(StubComponentFactory::new(FactoryBehaviour::Faithful)),
        ),
        Err(RenderImplementationError::DuplicateFactory {
            route_kind: RenderRouteKind::Component,
            ..
        })
    ));
    assert!(matches!(
        builder.capability_factory(
            "not a uri",
            Arc::new(StubCapabilityFactory::new(FactoryBehaviour::Faithful)),
        ),
        Err(RenderImplementationError::InvalidFactoryUri {
            route_kind: RenderRouteKind::Capability,
            ..
        })
    ));

    let registry = partial_registry(true, true, false);
    assert!(matches!(
        activate_render_world_contract(&fixture_document(), &registry),
        Err(RenderImplementationError::UnknownFactory {
            route_kind: RenderRouteKind::Capability,
            handler_factory_uri,
            ..
        }) if handler_factory_uri == MESH_FACTORY
    ));
}

#[test]
fn produced_handler_identity_and_revision_must_match_exactly() {
    let registry = custom_registry(
        FactoryBehaviour::Faithful,
        FactoryBehaviour::Drifted,
        FactoryBehaviour::Faithful,
        FactoryBehaviour::Faithful,
        FactoryBehaviour::Faithful,
    );
    assert!(matches!(
        activate_render_world_contract(&fixture_document(), &registry),
        Err(RenderImplementationError::HandlerMismatch {
            route_kind: RenderRouteKind::Component,
            expected_handler_id,
            expected_handler_revision: 2,
            observed,
            ..
        }) if expected_handler_id == "synthetic-mesh-handler"
            && observed.handler_id == DRIFTED_HANDLER_ID
            && observed.revision == DRIFTED_HANDLER_REVISION
    ));
}

#[test]
fn factory_refusal_preserves_exact_route_context() {
    let registry = custom_registry(
        FactoryBehaviour::Faithful,
        FactoryBehaviour::Faithful,
        FactoryBehaviour::Faithful,
        FactoryBehaviour::Faithful,
        FactoryBehaviour::Refuse,
    );
    assert!(matches!(
        activate_render_world_contract(&fixture_document(), &registry),
        Err(RenderImplementationError::FactoryRejected {
            route_kind: RenderRouteKind::Capability,
            route_key,
            handler_factory_uri,
            reason,
        }) if route_key == "synthetic:capability/terrain-height"
            && handler_factory_uri == HEIGHT_FACTORY
            && reason == REFUSAL
    ));
}

#[test]
fn shared_payload_contract_requires_one_exact_adapter_instance() {
    let document = document_with_shared_component_payload();
    assert!(matches!(
        activate_render_world_contract(&document, &faithful_registry()),
        Err(RenderImplementationError::ConflictingAdapter {
            route_kind: RenderRouteKind::Component,
            ..
        })
    ));

    let shared_factory = Arc::new(SharedAdapterComponentFactory {
        adapter: Arc::new(ProbeAdapter),
    });
    let mut builder = RenderImplementationRegistryBuilder::new();
    builder
        .document_factory(
            DOCUMENT_FACTORY,
            Arc::new(StubDocumentFactory::new(FactoryBehaviour::Faithful)),
        )
        .expect("document factory registers")
        .component_factory(MESH_FACTORY, shared_factory.clone())
        .expect("mesh component factory registers")
        .component_factory(WATER_FACTORY, shared_factory)
        .expect("water component factory registers")
        .capability_factory(
            MESH_FACTORY,
            Arc::new(StubCapabilityFactory::new(FactoryBehaviour::Faithful)),
        )
        .expect("mesh capability factory registers")
        .capability_factory(
            HEIGHT_FACTORY,
            Arc::new(StubCapabilityFactory::new(FactoryBehaviour::Faithful)),
        )
        .expect("height capability factory registers");
    assert!(activate_render_world_contract(&document, &builder.build()).is_ok());
}

#[test]
fn malformed_authored_document_is_rejected_before_lookup() {
    let mut value = valid_contract();
    value["routes"]["document"]["schema_version"] = json!(0);
    assert!(matches!(
        ContractDocument::from_json_bytes(&contract_bytes(&value)),
        Err(ContractDocumentError::OutOfRange {
            field: "document route schema_version",
            ..
        })
    ));
}
