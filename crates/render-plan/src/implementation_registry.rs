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

mod activation;
use crate::document::{
    CapabilityRouteDeclaration, ComponentRouteDeclaration, DocumentRouteDeclaration, RouteIdentity,
};
use crate::error::RenderImplementationError;
use crate::scene::{RouteSceneFactory, SceneCompositionFactory};
pub use activation::{
    ActivatedRenderPayloadContract, ActivatedRenderWorldContract, activate_render_payload_contract,
    activate_render_world_contract,
};

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
/// Render activation fails closed when the scene composition factory is absent.
/// Payload activation permits its absence without inventing a composer.
pub struct DocumentImplementation {
    handler: Arc<dyn DocumentHandler>,
    root_adapter: Arc<dyn RenderPayloadAdapter>,
    scene_composer: Option<Arc<dyn SceneCompositionFactory>>,
}

impl DocumentImplementation {
    /// Adopts one document handler and its root payload adapter.
    ///
    /// The returned bundle carries no scene composer. Render activation refuses
    /// it until [`Self::with_scene_composer`] supplies one; payload preparation
    /// does not require a composer.
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
    /// The returned bundle carries no route scene factory. Render activation
    /// refuses it until [`Self::with_scene_factory`] supplies one; payload
    /// preparation does not require scene factories.
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
    /// The returned bundle carries no route scene factory. Render activation
    /// refuses it until [`Self::with_scene_factory`] supplies one; payload
    /// preparation does not require scene factories.
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
