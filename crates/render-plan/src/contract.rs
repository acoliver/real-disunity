//! Caller-built immutable contract of exact render routes and payload adapters.
//!
//! The same registrations build the runtime registry and the adaptation closure,
//! so negotiation and CPU-plan adaptation cannot silently drift apart. There is
//! no default route, fallback, or content sniffing.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use worldrunner_world_model::{validate_semantic_identifier, validate_structural_uri};
use worldrunner_world_runtime::{
    CapabilityHandler, ComponentHandler, ContributionKind, DocumentHandler, HandlerDescriptor,
    RuntimeRegistry, RuntimeRegistryBuilder,
};

use crate::adapter::RenderPayloadAdapter;
use crate::error::RenderContractError;

/// Kinds of registration carried by the contract builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractRegistration {
    DocumentSchema,
    DocumentHandler,
    RootPayloadContract,
    ComponentAdapter,
    ResourceAdapter,
    ComponentRoute,
    CapabilityRoute,
}

pub(crate) struct ComponentRoute {
    handler: Arc<dyn ComponentHandler>,
    payload_contract_uri: String,
}

impl ComponentRoute {
    pub(crate) fn handler(&self) -> &Arc<dyn ComponentHandler> {
        &self.handler
    }

    pub(crate) fn payload_contract_uri(&self) -> &str {
        &self.payload_contract_uri
    }
}

pub(crate) struct CapabilityRoute {
    handler: Arc<dyn CapabilityHandler>,
    payload_contract_uri: String,
}

impl CapabilityRoute {
    pub(crate) fn handler(&self) -> &Arc<dyn CapabilityHandler> {
        &self.handler
    }

    pub(crate) fn payload_contract_uri(&self) -> &str {
        &self.payload_contract_uri
    }
}

/// Immutable set of exact runtime routes and CPU payload adapters.
pub struct RenderWorldContract {
    registry: RuntimeRegistry,
    document_handler: HandlerDescriptor,
    root_payload_contract_uri: String,
    root_adapter: Arc<dyn RenderPayloadAdapter>,
    component_routes: BTreeMap<String, ComponentRoute>,
    capability_routes: BTreeMap<String, CapabilityRoute>,
    component_adapters: BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
    resource_adapters: BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
    route_scoped_component_uris: BTreeSet<String>,
    route_scoped_resource_uris: BTreeSet<String>,
}

impl RenderWorldContract {
    #[must_use]
    pub fn runtime_registry(&self) -> &RuntimeRegistry {
        &self.registry
    }

    #[must_use]
    pub fn root_payload_contract_uri(&self) -> &str {
        &self.root_payload_contract_uri
    }

    pub(crate) fn document_handler_descriptor(&self) -> &HandlerDescriptor {
        &self.document_handler
    }

    pub(crate) fn root_adapter(&self) -> &Arc<dyn RenderPayloadAdapter> {
        &self.root_adapter
    }

    pub(crate) fn component_route(&self, schema_uri: &str) -> Option<&ComponentRoute> {
        self.component_routes.get(schema_uri)
    }

    pub(crate) fn capability_route(&self, capability_uri: &str) -> Option<&CapabilityRoute> {
        self.capability_routes.get(capability_uri)
    }

    pub(crate) fn component_adapter(
        &self,
        payload_contract_uri: &str,
    ) -> Option<&Arc<dyn RenderPayloadAdapter>> {
        self.component_adapters.get(payload_contract_uri)
    }

    pub(crate) fn resource_adapter(
        &self,
        payload_contract_uri: &str,
    ) -> Option<&Arc<dyn RenderPayloadAdapter>> {
        self.resource_adapters.get(payload_contract_uri)
    }

    pub(crate) fn is_route_scoped(
        &self,
        kind: ContributionKind,
        payload_contract_uri: &str,
    ) -> bool {
        match kind {
            ContributionKind::Component => self
                .route_scoped_component_uris
                .contains(payload_contract_uri),
            ContributionKind::Resource => self
                .route_scoped_resource_uris
                .contains(payload_contract_uri),
        }
    }

    pub(crate) fn component_adapter_uris(&self) -> impl Iterator<Item = &String> {
        self.component_adapters.keys()
    }

    pub(crate) fn resource_adapter_uris(&self) -> impl Iterator<Item = &String> {
        self.resource_adapters.keys()
    }

    pub(crate) fn component_route_keys(&self) -> impl Iterator<Item = &String> {
        self.component_routes.keys()
    }

    pub(crate) fn capability_route_keys(&self) -> impl Iterator<Item = &String> {
        self.capability_routes.keys()
    }
}

impl fmt::Debug for RenderWorldContract {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RenderWorldContract")
            .field("document_schema", self.registry.document_schema())
            .field("document_handler", &self.document_handler)
            .field("root_payload_contract_uri", &self.root_payload_contract_uri)
            .field("component_routes", &self.component_routes.keys())
            .field("capability_routes", &self.capability_routes.keys())
            .field("component_adapters", &self.component_adapters.keys())
            .field("resource_adapters", &self.resource_adapters.keys())
            .finish_non_exhaustive()
    }
}

/// Builder for one exact immutable render-world contract.
pub struct RenderWorldContractBuilder {
    document_schema: Option<(String, u32)>,
    document_handler: Option<Arc<dyn DocumentHandler>>,
    root: Option<RootRegistration>,
    component_adapters: BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
    resource_adapters: BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
    component_routes: BTreeMap<String, ComponentRoute>,
    capability_routes: BTreeMap<String, CapabilityRoute>,
}

struct RootRegistration {
    payload_contract_uri: String,
    adapter: Arc<dyn RenderPayloadAdapter>,
}

impl Default for RenderWorldContractBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderWorldContractBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            document_schema: None,
            document_handler: None,
            root: None,
            component_adapters: BTreeMap::new(),
            resource_adapters: BTreeMap::new(),
            component_routes: BTreeMap::new(),
            capability_routes: BTreeMap::new(),
        }
    }

    pub fn document_schema(
        &mut self,
        tag: &str,
        version: u32,
    ) -> Result<&mut Self, RenderContractError> {
        validate_uri(ContractRegistration::DocumentSchema, tag)?;
        reject_duplicate(
            ContractRegistration::DocumentSchema,
            tag,
            self.document_schema.is_some(),
        )?;
        self.document_schema = Some((tag.to_owned(), version));
        Ok(self)
    }

    pub fn document_handler(
        &mut self,
        handler: Arc<dyn DocumentHandler>,
    ) -> Result<&mut Self, RenderContractError> {
        validate_handler(ContractRegistration::DocumentHandler, handler.descriptor())?;
        reject_duplicate(
            ContractRegistration::DocumentHandler,
            "document-root",
            self.document_handler.is_some(),
        )?;
        self.document_handler = Some(handler);
        Ok(self)
    }

    pub fn root_payload_contract(
        &mut self,
        payload_contract_uri: &str,
        adapter: Arc<dyn RenderPayloadAdapter>,
    ) -> Result<&mut Self, RenderContractError> {
        validate_uri(
            ContractRegistration::RootPayloadContract,
            payload_contract_uri,
        )?;
        reject_duplicate(
            ContractRegistration::RootPayloadContract,
            payload_contract_uri,
            self.root.is_some(),
        )?;
        self.root = Some(RootRegistration {
            payload_contract_uri: payload_contract_uri.to_owned(),
            adapter,
        });
        Ok(self)
    }

    pub fn component_adapter(
        &mut self,
        payload_contract_uri: &str,
        adapter: Arc<dyn RenderPayloadAdapter>,
    ) -> Result<&mut Self, RenderContractError> {
        validate_uri(ContractRegistration::ComponentAdapter, payload_contract_uri)?;
        reject_duplicate(
            ContractRegistration::ComponentAdapter,
            payload_contract_uri,
            self.component_adapters.contains_key(payload_contract_uri),
        )?;
        self.component_adapters
            .insert(payload_contract_uri.to_owned(), adapter);
        Ok(self)
    }

    pub fn resource_adapter(
        &mut self,
        payload_contract_uri: &str,
        adapter: Arc<dyn RenderPayloadAdapter>,
    ) -> Result<&mut Self, RenderContractError> {
        validate_uri(ContractRegistration::ResourceAdapter, payload_contract_uri)?;
        reject_duplicate(
            ContractRegistration::ResourceAdapter,
            payload_contract_uri,
            self.resource_adapters.contains_key(payload_contract_uri),
        )?;
        self.resource_adapters
            .insert(payload_contract_uri.to_owned(), adapter);
        Ok(self)
    }

    pub fn component_route(
        &mut self,
        schema_uri: &str,
        handler: Arc<dyn ComponentHandler>,
        expected_payload_contract_uri: &str,
    ) -> Result<&mut Self, RenderContractError> {
        validate_uri(ContractRegistration::ComponentRoute, schema_uri)?;
        validate_uri(
            ContractRegistration::ComponentRoute,
            expected_payload_contract_uri,
        )?;
        validate_handler(ContractRegistration::ComponentRoute, handler.descriptor())?;
        reject_duplicate(
            ContractRegistration::ComponentRoute,
            schema_uri,
            self.component_routes.contains_key(schema_uri),
        )?;
        self.component_routes.insert(
            schema_uri.to_owned(),
            ComponentRoute {
                handler,
                payload_contract_uri: expected_payload_contract_uri.to_owned(),
            },
        );
        Ok(self)
    }

    pub fn capability_route(
        &mut self,
        capability_uri: &str,
        handler: Arc<dyn CapabilityHandler>,
        expected_payload_contract_uri: &str,
    ) -> Result<&mut Self, RenderContractError> {
        validate_uri(ContractRegistration::CapabilityRoute, capability_uri)?;
        validate_uri(
            ContractRegistration::CapabilityRoute,
            expected_payload_contract_uri,
        )?;
        validate_handler(ContractRegistration::CapabilityRoute, handler.descriptor())?;
        reject_duplicate(
            ContractRegistration::CapabilityRoute,
            capability_uri,
            self.capability_routes.contains_key(capability_uri),
        )?;
        self.capability_routes.insert(
            capability_uri.to_owned(),
            CapabilityRoute {
                handler,
                payload_contract_uri: expected_payload_contract_uri.to_owned(),
            },
        );
        Ok(self)
    }

    pub fn build(self) -> Result<RenderWorldContract, RenderContractError> {
        let schema = self
            .document_schema
            .ok_or_else(|| missing(ContractRegistration::DocumentSchema))?;
        let document_handler = self
            .document_handler
            .ok_or_else(|| missing(ContractRegistration::DocumentHandler))?;
        let root = self
            .root
            .ok_or_else(|| missing(ContractRegistration::RootPayloadContract))?;
        let component_uris = route_contracts(&self.component_routes);
        let resource_uris = route_contracts(&self.capability_routes);
        verify_adapters(
            ContractRegistration::ComponentRoute,
            ContractRegistration::ComponentAdapter,
            &component_uris,
            &self.component_adapters,
        )?;
        verify_adapters(
            ContractRegistration::CapabilityRoute,
            ContractRegistration::ResourceAdapter,
            &resource_uris,
            &self.resource_adapters,
        )?;
        reject_orphans(
            ContributionKind::Component,
            &component_uris,
            &self.component_adapters,
        )?;
        reject_orphans(
            ContributionKind::Resource,
            &resource_uris,
            &self.resource_adapters,
        )?;
        let document_descriptor = document_handler.descriptor().clone();
        let registry = build_registry(
            &schema,
            document_handler,
            &self.component_routes,
            &self.capability_routes,
        )?;
        Ok(RenderWorldContract {
            registry,
            document_handler: document_descriptor,
            root_payload_contract_uri: root.payload_contract_uri,
            root_adapter: root.adapter,
            component_routes: self.component_routes,
            capability_routes: self.capability_routes,
            component_adapters: self.component_adapters,
            resource_adapters: self.resource_adapters,
            route_scoped_component_uris: component_uris,
            route_scoped_resource_uris: resource_uris,
        })
    }
}

trait RouteContract {
    fn payload_contract_uri(&self) -> &str;
}

impl RouteContract for ComponentRoute {
    fn payload_contract_uri(&self) -> &str {
        self.payload_contract_uri()
    }
}

impl RouteContract for CapabilityRoute {
    fn payload_contract_uri(&self) -> &str {
        self.payload_contract_uri()
    }
}

fn route_contracts<T>(routes: &BTreeMap<String, T>) -> BTreeSet<String>
where
    T: RouteContract,
{
    routes
        .values()
        .map(|route| route.payload_contract_uri().to_owned())
        .collect()
}

fn verify_adapters(
    route_kind: ContractRegistration,
    adapter_kind: ContractRegistration,
    contracts: &BTreeSet<String>,
    adapters: &BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
) -> Result<(), RenderContractError> {
    for contract in contracts {
        if !adapters.contains_key(contract) {
            return Err(RenderContractError::UnresolvedAdapter {
                route_kind,
                route_key: contract.clone(),
                adapter_kind,
                payload_contract_uri: contract.clone(),
            });
        }
    }
    Ok(())
}

fn reject_orphans(
    kind: ContributionKind,
    contracts: &BTreeSet<String>,
    adapters: &BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
) -> Result<(), RenderContractError> {
    if let Some(uri) = adapters.keys().find(|uri| !contracts.contains(*uri)) {
        return Err(RenderContractError::OrphanAdapter {
            kind,
            payload_contract_uri: uri.clone(),
        });
    }
    Ok(())
}

fn build_registry(
    schema: &(String, u32),
    document_handler: Arc<dyn DocumentHandler>,
    component_routes: &BTreeMap<String, ComponentRoute>,
    capability_routes: &BTreeMap<String, CapabilityRoute>,
) -> Result<RuntimeRegistry, RenderContractError> {
    let mut builder = RuntimeRegistryBuilder::new();
    builder
        .document_schema(&schema.0, schema.1)?
        .document_handler(document_handler)?;
    for (schema_uri, route) in component_routes {
        builder.component_handler(schema_uri, Arc::clone(route.handler()))?;
    }
    for (capability_uri, route) in capability_routes {
        builder.capability_handler(capability_uri, Arc::clone(route.handler()))?;
    }
    builder.build().map_err(RenderContractError::Registry)
}

fn validate_uri(kind: ContractRegistration, key: &str) -> Result<(), RenderContractError> {
    validate_structural_uri(key).map_err(|source| RenderContractError::InvalidKey {
        kind,
        key: key.to_owned(),
        source,
    })
}

fn validate_handler(
    kind: ContractRegistration,
    descriptor: &HandlerDescriptor,
) -> Result<(), RenderContractError> {
    validate_semantic_identifier(&descriptor.handler_id).map_err(|source| {
        RenderContractError::InvalidKey {
            kind,
            key: descriptor.handler_id.clone(),
            source,
        }
    })
}

fn reject_duplicate(
    kind: ContractRegistration,
    key: &str,
    duplicate: bool,
) -> Result<(), RenderContractError> {
    if duplicate {
        return Err(RenderContractError::DuplicateRegistration {
            kind,
            key: key.to_owned(),
        });
    }
    Ok(())
}

fn missing(kind: ContractRegistration) -> RenderContractError {
    RenderContractError::MissingRegistration { kind }
}
