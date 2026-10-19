//! Descriptor-keyed registry turning authored route declarations into exact
//! render-world contracts.
//!
//! Activation is generic over authored identities: it looks up each route
//! declaration's exact `handler_factory_uri` in one caller-built
//! [`RenderImplementationRegistry`], invokes that factory with the exact
//! declaration, verifies the produced handler descriptor matches the authored
//! handler identity and revision, and wires the authored schema, payload
//! contract, handler, route, and adapter through
//! [`crate::RenderWorldContractBuilder`]. There is no default factory, no
//! fallback route, no similarly-spelled-URI tolerance, no content sniffing,
//! and no dataset knowledge: factories are registered separately per route
//! kind under exact URIs, an unknown present declaration fails closed, and a
//! URI registered for one route kind is invisible to the other kinds.
//!
//! Factories receive the whole declaration, so the authored role URI,
//! cardinality bounds, and capability
//! [`BindingElementPolicy`](crate::document::BindingElementPolicy) are
//! available to them verbatim; generic activation itself never interprets
//! role values or element policies. Activation is also not a whole-plan gate: it binds
//! one implementation per declared route, while population of the declared
//! cardinalities and the role completeness of the publication's resource
//! bindings are validated later, at render-plan build time, against the
//! publication the contract consumes.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use thiserror::Error;
use worldrunner_world_model::validate_structural_uri;
use worldrunner_world_runtime::{
    CapabilityHandler, ComponentHandler, DocumentHandler, HandlerDescriptor,
};

use crate::adapter::RenderPayloadAdapter;
use crate::contract::{RenderWorldContract, RenderWorldContractBuilder};
use crate::document::{
    CapabilityRouteDeclaration, ComponentRouteDeclaration, ContractDocument,
    DocumentRouteDeclaration, RouteIdentity,
};
use crate::error::RenderImplementationError;
use crate::scene::{RouteSceneFactory, SceneCompositionFactory};

/// The route kinds a registry binds implementations for.
///
/// Registrations are keyed separately per kind: a factory URI registered for
/// one kind is invisible to the other kinds, so activation can never fall
/// back across kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderRouteKind {
    /// The single document route.
    Document,
    /// Component routes keyed by component schema URI.
    Component,
    /// Capability routes keyed by capability URI.
    Capability,
}

/// Typed refusal of one declaration by an implementation factory.
///
/// The factory reports only its own reason; activation preserves the exact
/// route kind, route key, and factory URI around it, so factory failures
/// never need boxed or borrowed error chains.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("implementation factory refused the declaration: {reason:?}")]
pub struct RenderFactoryError {
    reason: String,
}

impl RenderFactoryError {
    /// Creates one typed factory refusal carrying the refusal reason.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// Returns the factory's refusal reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Document-route implementation bundle produced by one factory.
///
/// The bundle carries the executable parts of the route: the root document
/// handler, the adapter minting the root payload contract's CPU state, and
/// the document scene composition factory. Every authored identity (schema,
/// payload contract, handler descriptor) stays owned by the declaration.
/// Activation fails closed when the scene composition factory is absent, so
/// an activated document always retains one exact composer.
pub struct DocumentImplementation {
    handler: Arc<dyn DocumentHandler>,
    root_adapter: Arc<dyn RenderPayloadAdapter>,
    scene_composer: Option<Arc<dyn SceneCompositionFactory>>,
}

impl DocumentImplementation {
    /// Adopts one document handler and its root payload adapter.
    ///
    /// The returned bundle carries no scene composer yet; activation refuses
    /// it until [`Self::with_scene_composer`] supplies one.
    #[must_use]
    pub fn new(
        handler: Arc<dyn DocumentHandler>,
        root_adapter: Arc<dyn RenderPayloadAdapter>,
    ) -> Self {
        Self {
            handler,
            root_adapter,
            scene_composer: None,
        }
    }

    /// Binds the document scene composition factory.
    #[must_use]
    pub fn with_scene_composer(mut self, composer: Arc<dyn SceneCompositionFactory>) -> Self {
        self.scene_composer = Some(composer);
        self
    }

    /// Returns the root document handler.
    #[must_use]
    pub fn handler(&self) -> &Arc<dyn DocumentHandler> {
        &self.handler
    }

    /// Returns the adapter for the authored root payload contract.
    #[must_use]
    pub fn root_adapter(&self) -> &Arc<dyn RenderPayloadAdapter> {
        &self.root_adapter
    }

    /// Returns the document scene composition factory, when bound.
    #[must_use]
    pub const fn scene_composer(&self) -> Option<&Arc<dyn SceneCompositionFactory>> {
        self.scene_composer.as_ref()
    }
}

impl fmt::Debug for DocumentImplementation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DocumentImplementation")
            .field("handler", self.handler.descriptor())
            .finish_non_exhaustive()
    }
}

/// Component-route implementation bundle produced by one factory.
pub struct ComponentImplementation {
    handler: Arc<dyn ComponentHandler>,
    adapter: Arc<dyn RenderPayloadAdapter>,
    scene_factory: Option<Arc<dyn RouteSceneFactory>>,
}

impl ComponentImplementation {
    /// Adopts one component handler and its payload adapter.
    ///
    /// The returned bundle carries no route scene factory yet; activation
    /// refuses it until [`Self::with_scene_factory`] supplies one.
    #[must_use]
    pub fn new(handler: Arc<dyn ComponentHandler>, adapter: Arc<dyn RenderPayloadAdapter>) -> Self {
        Self {
            handler,
            adapter,
            scene_factory: None,
        }
    }

    /// Binds the route scene contribution factory of this component route.
    #[must_use]
    pub fn with_scene_factory(mut self, factory: Arc<dyn RouteSceneFactory>) -> Self {
        self.scene_factory = Some(factory);
        self
    }

    /// Returns the component handler.
    #[must_use]
    pub fn handler(&self) -> &Arc<dyn ComponentHandler> {
        &self.handler
    }

    /// Returns the adapter for the authored payload contract.
    #[must_use]
    pub fn adapter(&self) -> &Arc<dyn RenderPayloadAdapter> {
        &self.adapter
    }

    /// Returns the route scene contribution factory, when bound.
    #[must_use]
    pub const fn scene_factory(&self) -> Option<&Arc<dyn RouteSceneFactory>> {
        self.scene_factory.as_ref()
    }
}

impl fmt::Debug for ComponentImplementation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentImplementation")
            .field("handler", self.handler.descriptor())
            .finish_non_exhaustive()
    }
}

/// Capability-route implementation bundle produced by one factory.
pub struct CapabilityImplementation {
    handler: Arc<dyn CapabilityHandler>,
    adapter: Arc<dyn RenderPayloadAdapter>,
    scene_factory: Option<Arc<dyn RouteSceneFactory>>,
}

impl CapabilityImplementation {
    /// Adopts one capability handler and its payload adapter.
    ///
    /// The returned bundle carries no route scene factory yet; activation
    /// refuses it until [`Self::with_scene_factory`] supplies one.
    #[must_use]
    pub fn new(
        handler: Arc<dyn CapabilityHandler>,
        adapter: Arc<dyn RenderPayloadAdapter>,
    ) -> Self {
        Self {
            handler,
            adapter,
            scene_factory: None,
        }
    }

    /// Binds the route scene contribution factory of this capability route.
    #[must_use]
    pub fn with_scene_factory(mut self, factory: Arc<dyn RouteSceneFactory>) -> Self {
        self.scene_factory = Some(factory);
        self
    }

    /// Returns the capability handler.
    #[must_use]
    pub fn handler(&self) -> &Arc<dyn CapabilityHandler> {
        &self.handler
    }

    /// Returns the adapter for the authored payload contract.
    #[must_use]
    pub fn adapter(&self) -> &Arc<dyn RenderPayloadAdapter> {
        &self.adapter
    }

    /// Returns the route scene contribution factory, when bound.
    #[must_use]
    pub const fn scene_factory(&self) -> Option<&Arc<dyn RouteSceneFactory>> {
        self.scene_factory.as_ref()
    }
}

impl fmt::Debug for CapabilityImplementation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityImplementation")
            .field("handler", self.handler.descriptor())
            .finish_non_exhaustive()
    }
}

/// Builds the document-route implementation for one exact declaration.
///
/// The factory receives the exact authored
/// [`DocumentRouteDeclaration`], including the authored role URI, schema
/// identity, handler identity and revision, payload contract URI, and
/// handler factory URI. Activation verifies the produced handler descriptor
/// against the authored identity afterwards.
pub trait DocumentImplementationFactory: Send + Sync {
    /// Builds the document implementation for one exact declaration.
    ///
    /// # Errors
    ///
    /// Returns [`RenderFactoryError`] when this factory cannot serve the
    /// exact declaration; activation fails closed with the route context.
    fn build_implementation(
        &self,
        declaration: &DocumentRouteDeclaration,
    ) -> Result<DocumentImplementation, RenderFactoryError>;
}

/// Builds the component-route implementation for one exact declaration.
///
/// The factory receives the exact authored
/// [`ComponentRouteDeclaration`], including the authored role URI, schema
/// URI, cardinality bounds, handler identity and revision, payload contract
/// URI, and handler factory URI.
pub trait ComponentImplementationFactory: Send + Sync {
    /// Builds the component implementation for one exact declaration.
    ///
    /// # Errors
    ///
    /// Returns [`RenderFactoryError`] when this factory cannot serve the
    /// exact declaration; activation fails closed with the route context.
    fn build_implementation(
        &self,
        declaration: &ComponentRouteDeclaration,
    ) -> Result<ComponentImplementation, RenderFactoryError>;
}

/// Builds the capability-route implementation for one exact declaration.
///
/// The factory receives the exact authored
/// [`CapabilityRouteDeclaration`], including the authored role URI,
/// capability URI, cardinality bounds,
/// [`BindingElementPolicy`](crate::document::BindingElementPolicy), handler
/// identity and revision, payload contract URI, and handler factory URI.
/// Activation binds one implementation per declared route only; whether a
/// publication's bound content actually populates the declared cardinalities
/// and element policies is validated later at render-plan build time.
pub trait CapabilityImplementationFactory: Send + Sync {
    /// Builds the capability implementation for one exact declaration.
    ///
    /// # Errors
    ///
    /// Returns [`RenderFactoryError`] when this factory cannot serve the
    /// exact declaration; activation fails closed with the route context.
    fn build_implementation(
        &self,
        declaration: &CapabilityRouteDeclaration,
    ) -> Result<CapabilityImplementation, RenderFactoryError>;
}

/// Immutable, caller-built registry of exact implementation factories.
///
/// Factories are keyed by exact handler factory URI, separately per route
/// kind, and only through [`RenderImplementationRegistryBuilder`]. An empty
/// registry is legal: activation then rejects every present declaration
/// with [`RenderImplementationError::UnknownFactory`].
pub struct RenderImplementationRegistry {
    documents: BTreeMap<String, Arc<dyn DocumentImplementationFactory>>,
    components: BTreeMap<String, Arc<dyn ComponentImplementationFactory>>,
    capabilities: BTreeMap<String, Arc<dyn CapabilityImplementationFactory>>,
}

impl fmt::Debug for RenderImplementationRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RenderImplementationRegistry")
            .field("documents", &self.documents.keys())
            .field("components", &self.components.keys())
            .field("capabilities", &self.capabilities.keys())
            .finish_non_exhaustive()
    }
}

/// Builder for one immutable [`RenderImplementationRegistry`].
///
/// Each registration names one exact handler factory URI and is stored
/// under its own route kind, so the same URI may legitimately serve
/// different kinds while duplicates within one kind fail typed.
pub struct RenderImplementationRegistryBuilder {
    documents: BTreeMap<String, Arc<dyn DocumentImplementationFactory>>,
    components: BTreeMap<String, Arc<dyn ComponentImplementationFactory>>,
    capabilities: BTreeMap<String, Arc<dyn CapabilityImplementationFactory>>,
}

impl Default for RenderImplementationRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderImplementationRegistryBuilder {
    /// Creates one empty registry builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            documents: BTreeMap::new(),
            components: BTreeMap::new(),
            capabilities: BTreeMap::new(),
        }
    }

    /// Registers one document-route factory under an exact URI.
    ///
    /// # Errors
    ///
    /// Returns [`RenderImplementationError`] when the URI is malformed or
    /// already carries a document factory registration.
    pub fn document_factory(
        &mut self,
        handler_factory_uri: &str,
        factory: Arc<dyn DocumentImplementationFactory>,
    ) -> Result<&mut Self, RenderImplementationError> {
        register_factory(
            &mut self.documents,
            RenderRouteKind::Document,
            handler_factory_uri,
            factory,
        )?;
        Ok(self)
    }

    /// Registers one component-route factory under an exact URI.
    ///
    /// # Errors
    ///
    /// Returns [`RenderImplementationError`] when the URI is malformed or
    /// already carries a component factory registration.
    pub fn component_factory(
        &mut self,
        handler_factory_uri: &str,
        factory: Arc<dyn ComponentImplementationFactory>,
    ) -> Result<&mut Self, RenderImplementationError> {
        register_factory(
            &mut self.components,
            RenderRouteKind::Component,
            handler_factory_uri,
            factory,
        )?;
        Ok(self)
    }

    /// Registers one capability-route factory under an exact URI.
    ///
    /// # Errors
    ///
    /// Returns [`RenderImplementationError`] when the URI is malformed or
    /// already carries a capability factory registration.
    pub fn capability_factory(
        &mut self,
        handler_factory_uri: &str,
        factory: Arc<dyn CapabilityImplementationFactory>,
    ) -> Result<&mut Self, RenderImplementationError> {
        register_factory(
            &mut self.capabilities,
            RenderRouteKind::Capability,
            handler_factory_uri,
            factory,
        )?;
        Ok(self)
    }

    /// Freezes the registrations into one immutable registry.
    #[must_use]
    pub fn build(self) -> RenderImplementationRegistry {
        RenderImplementationRegistry {
            documents: self.documents,
            components: self.components,
            capabilities: self.capabilities,
        }
    }
}

fn register_factory<F: ?Sized>(
    factories: &mut BTreeMap<String, Arc<F>>,
    route_kind: RenderRouteKind,
    handler_factory_uri: &str,
    factory: Arc<F>,
) -> Result<(), RenderImplementationError> {
    validate_structural_uri(handler_factory_uri).map_err(|source| {
        RenderImplementationError::InvalidFactoryUri {
            route_kind,
            handler_factory_uri: handler_factory_uri.to_owned(),
            source,
        }
    })?;
    if factories.contains_key(handler_factory_uri) {
        return Err(RenderImplementationError::DuplicateFactory {
            route_kind,
            handler_factory_uri: handler_factory_uri.to_owned(),
        });
    }
    factories.insert(handler_factory_uri.to_owned(), factory);
    Ok(())
}

/// Adapter map one activation binds payload contracts into.
#[derive(Clone, Copy)]
enum AdapterBinding {
    Component,
    Resource,
}

/// Scene factories one activation binds, keyed per route kind.
///
/// The document composer and every route scene factory are retained together
/// with the executable contract so plan building invokes the exact
/// activation-bound scene phase without any second registry or lookup by
/// role.
struct SceneFactoryBinding {
    document_composer: Arc<dyn SceneCompositionFactory>,
    component: BTreeMap<String, Arc<dyn RouteSceneFactory>>,
    capability: BTreeMap<String, Arc<dyn RouteSceneFactory>>,
}

/// Exact authored document bound to its activated executable implementation.
///
/// Values can only be minted by [`activate_render_world_contract`]. Keeping the
/// validated authored declarations, executable contract, and activation-bound
/// scene factories together prevents a caller from substituting a different
/// document or scene phase during whole-plan closure.
pub struct ActivatedRenderWorldContract {
    authored: ContractDocument,
    executable: RenderWorldContract,
    scene: SceneFactoryBinding,
}

impl ActivatedRenderWorldContract {
    /// Returns the exact validated document used during activation.
    #[must_use]
    pub const fn authored_contract(&self) -> &ContractDocument {
        &self.authored
    }

    pub(crate) const fn executable_contract(&self) -> &RenderWorldContract {
        &self.executable
    }

    /// Returns the activation-bound document scene composition factory.
    pub(crate) const fn document_scene_composer(&self) -> &Arc<dyn SceneCompositionFactory> {
        &self.scene.document_composer
    }

    /// Returns the activation-bound scene factory of one component route.
    pub(crate) fn component_scene_factory(
        &self,
        schema_uri: &str,
    ) -> Option<&Arc<dyn RouteSceneFactory>> {
        self.scene.component.get(schema_uri)
    }

    /// Returns the activation-bound scene factory of one capability route.
    pub(crate) fn capability_scene_factory(
        &self,
        capability_uri: &str,
    ) -> Option<&Arc<dyn RouteSceneFactory>> {
        self.scene.capability.get(capability_uri)
    }
}

impl std::ops::Deref for ActivatedRenderWorldContract {
    type Target = RenderWorldContract;

    fn deref(&self) -> &Self::Target {
        &self.executable
    }
}

/// Activates one authored contract document into an exact executable contract.
///
/// The document is revalidated first, then every present route declaration
/// is resolved against `registry` by its exact `handler_factory_uri` under
/// its own route kind: the factory is invoked with the exact declaration,
/// the produced handler descriptor must match the authored handler identity
/// and revision exactly, the implementation must carry its route's scene
/// factory, and the authored schema, payload contract, handler, route, and
/// adapter are wired through [`RenderWorldContractBuilder`]. Nothing is
/// defaulted, substituted, sniffed, or shared across route kinds. Binding
/// implementations says nothing about publication population:
/// whether the publication's components and resource bindings populate the
/// declared cardinalities, element policies, and roles is enforced later at
/// render-plan build time.
///
/// # Errors
///
/// Returns [`RenderImplementationError`] when the document is malformed, a
/// present declaration names an unregistered factory, a factory refuses or
/// drifts from the authored handler identity, an implementation lacks its
/// route's scene factory, two routes claim one payload contract with
/// distinct adapters, or the contract builder rejects a wired registration.
pub fn activate_render_world_contract(
    document: &ContractDocument,
    registry: &RenderImplementationRegistry,
) -> Result<ActivatedRenderWorldContract, RenderImplementationError> {
    document.validate()?;
    let mut builder = RenderWorldContractBuilder::new();
    let mut component_adapters = BTreeMap::new();
    let mut resource_adapters = BTreeMap::new();
    let mut component_scene = BTreeMap::new();
    let mut capability_scene = BTreeMap::new();
    let document_composer =
        activate_document_route(document.routes().document(), registry, &mut builder)?;
    for route in document.routes().components() {
        let factory =
            activate_component_route(route, registry, &mut builder, &mut component_adapters)?;
        component_scene.insert(route.schema_uri().to_owned(), factory);
    }
    for route in document.routes().capabilities() {
        let factory =
            activate_capability_route(route, registry, &mut builder, &mut resource_adapters)?;
        capability_scene.insert(route.capability_uri().to_owned(), factory);
    }
    let executable = builder.build().map_err(RenderImplementationError::from)?;
    Ok(ActivatedRenderWorldContract {
        authored: document.clone(),
        executable,
        scene: SceneFactoryBinding {
            document_composer,
            component: component_scene,
            capability: capability_scene,
        },
    })
}

/// Resolves one exact declaration's factory under its own route kind.
fn resolve_factory<'a, F: ?Sized>(
    factories: &'a BTreeMap<String, Arc<F>>,
    route_kind: RenderRouteKind,
    route_key: &str,
    identity: &RouteIdentity,
) -> Result<&'a Arc<F>, RenderImplementationError> {
    factories
        .get(identity.handler_factory_uri())
        .ok_or_else(|| RenderImplementationError::UnknownFactory {
            route_kind,
            route_key: route_key.to_owned(),
            handler_factory_uri: identity.handler_factory_uri().to_owned(),
        })
}

/// Wraps one typed factory refusal with the exact route context.
fn factory_rejected(
    route_kind: RenderRouteKind,
    route_key: &str,
    identity: &RouteIdentity,
    source: &RenderFactoryError,
) -> RenderImplementationError {
    RenderImplementationError::FactoryRejected {
        route_kind,
        route_key: route_key.to_owned(),
        handler_factory_uri: identity.handler_factory_uri().to_owned(),
        reason: source.reason().to_owned(),
    }
}

/// Requires one produced handler descriptor to match the authored identity.
fn verify_authored_handler(
    route_kind: RenderRouteKind,
    route_key: &str,
    identity: &RouteIdentity,
    observed: &HandlerDescriptor,
) -> Result<(), RenderImplementationError> {
    if observed.handler_id == identity.handler_id()
        && observed.revision == identity.handler_revision()
    {
        return Ok(());
    }
    Err(RenderImplementationError::HandlerMismatch {
        route_kind,
        route_key: route_key.to_owned(),
        handler_factory_uri: identity.handler_factory_uri().to_owned(),
        expected_handler_id: identity.handler_id().to_owned(),
        expected_handler_revision: identity.handler_revision(),
        observed: observed.clone(),
    })
}

/// Binds one payload-contract adapter, tolerating only deliberate sharing.
///
/// The underlying contract builder allows several routes of one kind to
/// share a payload contract with a single adapter. When a later declaration
/// repeats an already-bound payload contract, its factory must return the
/// very same adapter instance; anything else fails closed rather than
/// silently choosing between the produced adapters.
#[derive(Clone, Copy)]
struct AdapterRoute<'a> {
    binding: AdapterBinding,
    route_kind: RenderRouteKind,
    route_key: &'a str,
    payload_contract_uri: &'a str,
}

fn bind_shared_adapter(
    builder: &mut RenderWorldContractBuilder,
    bound: &mut BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
    route: AdapterRoute<'_>,
    adapter: Arc<dyn RenderPayloadAdapter>,
) -> Result<(), RenderImplementationError> {
    if let Some(existing) = bound.get(route.payload_contract_uri) {
        if Arc::ptr_eq(existing, &adapter) {
            return Ok(());
        }
        return Err(RenderImplementationError::ConflictingAdapter {
            route_kind: route.route_kind,
            route_key: route.route_key.to_owned(),
            payload_contract_uri: route.payload_contract_uri.to_owned(),
        });
    }
    match route.binding {
        AdapterBinding::Component => {
            builder.component_adapter(route.payload_contract_uri, Arc::clone(&adapter))?
        }
        AdapterBinding::Resource => {
            builder.resource_adapter(route.payload_contract_uri, Arc::clone(&adapter))?
        }
    };
    bound.insert(route.payload_contract_uri.to_owned(), adapter);
    Ok(())
}

/// Requires one implementation to carry its route's scene factory.
fn missing_scene_factory(
    route_kind: RenderRouteKind,
    route_key: &str,
    identity: &RouteIdentity,
) -> RenderImplementationError {
    RenderImplementationError::MissingSceneFactory {
        route_kind,
        route_key: route_key.to_owned(),
        handler_factory_uri: identity.handler_factory_uri().to_owned(),
    }
}

/// Activates the mandatory document route.
fn activate_document_route(
    route: &DocumentRouteDeclaration,
    registry: &RenderImplementationRegistry,
    builder: &mut RenderWorldContractBuilder,
) -> Result<Arc<dyn SceneCompositionFactory>, RenderImplementationError> {
    let identity = route.identity();
    let factory = resolve_factory(
        &registry.documents,
        RenderRouteKind::Document,
        route.schema_uri(),
        identity,
    )?;
    let implementation = factory.build_implementation(route).map_err(|source| {
        factory_rejected(
            RenderRouteKind::Document,
            route.schema_uri(),
            identity,
            &source,
        )
    })?;
    verify_authored_handler(
        RenderRouteKind::Document,
        route.schema_uri(),
        identity,
        implementation.handler.descriptor(),
    )?;
    let composer = implementation.scene_composer().cloned().ok_or_else(|| {
        missing_scene_factory(RenderRouteKind::Document, route.schema_uri(), identity)
    })?;
    builder
        .document_schema(route.schema_uri(), route.schema_version())?
        .document_handler(implementation.handler)?
        .root_payload_contract(identity.payload_contract_uri(), implementation.root_adapter)?;
    Ok(composer)
}

/// Activates one declared component route.
fn activate_component_route(
    route: &ComponentRouteDeclaration,
    registry: &RenderImplementationRegistry,
    builder: &mut RenderWorldContractBuilder,
    bound_adapters: &mut BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
) -> Result<Arc<dyn RouteSceneFactory>, RenderImplementationError> {
    let identity = route.identity();
    let factory = resolve_factory(
        &registry.components,
        RenderRouteKind::Component,
        route.schema_uri(),
        identity,
    )?;
    let implementation = factory.build_implementation(route).map_err(|source| {
        factory_rejected(
            RenderRouteKind::Component,
            route.schema_uri(),
            identity,
            &source,
        )
    })?;
    verify_authored_handler(
        RenderRouteKind::Component,
        route.schema_uri(),
        identity,
        implementation.handler.descriptor(),
    )?;
    let scene_factory = implementation.scene_factory().cloned().ok_or_else(|| {
        missing_scene_factory(RenderRouteKind::Component, route.schema_uri(), identity)
    })?;
    let payload_contract_uri = identity.payload_contract_uri();
    bind_shared_adapter(
        builder,
        bound_adapters,
        AdapterRoute {
            binding: AdapterBinding::Component,
            route_kind: RenderRouteKind::Component,
            route_key: route.schema_uri(),
            payload_contract_uri,
        },
        implementation.adapter,
    )?;
    builder.component_route(
        route.schema_uri(),
        implementation.handler,
        payload_contract_uri,
    )?;
    Ok(scene_factory)
}

/// Activates one declared capability route.
fn activate_capability_route(
    route: &CapabilityRouteDeclaration,
    registry: &RenderImplementationRegistry,
    builder: &mut RenderWorldContractBuilder,
    bound_adapters: &mut BTreeMap<String, Arc<dyn RenderPayloadAdapter>>,
) -> Result<Arc<dyn RouteSceneFactory>, RenderImplementationError> {
    let identity = route.identity();
    let factory = resolve_factory(
        &registry.capabilities,
        RenderRouteKind::Capability,
        route.capability_uri(),
        identity,
    )?;
    let implementation = factory.build_implementation(route).map_err(|source| {
        factory_rejected(
            RenderRouteKind::Capability,
            route.capability_uri(),
            identity,
            &source,
        )
    })?;
    verify_authored_handler(
        RenderRouteKind::Capability,
        route.capability_uri(),
        identity,
        implementation.handler.descriptor(),
    )?;
    let scene_factory = implementation.scene_factory().cloned().ok_or_else(|| {
        missing_scene_factory(
            RenderRouteKind::Capability,
            route.capability_uri(),
            identity,
        )
    })?;
    let payload_contract_uri = identity.payload_contract_uri();
    bind_shared_adapter(
        builder,
        bound_adapters,
        AdapterRoute {
            binding: AdapterBinding::Resource,
            route_kind: RenderRouteKind::Capability,
            route_key: route.capability_uri(),
            payload_contract_uri,
        },
        implementation.adapter,
    )?;
    builder.capability_route(
        route.capability_uri(),
        implementation.handler,
        payload_contract_uri,
    )?;
    Ok(scene_factory)
}
