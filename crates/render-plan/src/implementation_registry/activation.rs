//! Exact handler activation and scene-phase admission.

use super::{
    Arc, BTreeMap, CapabilityRouteDeclaration, ComponentRouteDeclaration, DocumentRouteDeclaration,
    HandlerDescriptor, RenderFactoryError, RenderImplementationError, RenderImplementationRegistry,
    RenderPayloadAdapter, RenderRouteKind, RenderWorldContract, RenderWorldContractBuilder,
    RouteIdentity, RouteSceneFactory, SceneCompositionFactory,
};
use crate::document::ContractDocument;

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
    document_composer: Option<Arc<dyn SceneCompositionFactory>>,
    component: BTreeMap<String, Option<Arc<dyn RouteSceneFactory>>>,
    capability: BTreeMap<String, Option<Arc<dyn RouteSceneFactory>>>,
}

/// Exact authored routes bound to payload handlers and adapters.
///
/// Payload activation does not require or invoke scene factories. The same
/// binding can be admitted for rendering with [`Self::into_render_contract`].
pub struct ActivatedRenderPayloadContract {
    authored: ContractDocument,
    executable: RenderWorldContract,
    identity: Arc<()>,
    scene: SceneFactoryBinding,
}

impl ActivatedRenderPayloadContract {
    /// Returns the exact validated authored document.
    #[must_use]
    pub const fn authored_contract(&self) -> &ContractDocument {
        &self.authored
    }

    pub(crate) fn executable_contract(&self) -> &RenderWorldContract {
        &self.executable
    }

    pub(crate) const fn activation_identity(&self) -> &Arc<()> {
        &self.identity
    }

    /// Admits the bound factories for mandatory scene composition.
    ///
    /// # Errors
    /// Returns [`RenderImplementationError::MissingSceneFactory`] for the
    /// first declared route without its required scene factory.
    pub fn into_render_contract(
        self,
    ) -> Result<ActivatedRenderWorldContract, RenderImplementationError> {
        let document = self.authored.routes().document();
        let composer = self.scene.document_composer.clone().ok_or_else(|| {
            missing_scene_factory(
                RenderRouteKind::Document,
                document.schema_uri(),
                document.identity(),
            )
        })?;
        for route in self.authored.routes().components() {
            if self
                .scene
                .component
                .get(route.schema_uri())
                .and_then(Option::as_ref)
                .is_none()
            {
                return Err(missing_scene_factory(
                    RenderRouteKind::Component,
                    route.schema_uri(),
                    route.identity(),
                ));
            }
        }
        for route in self.authored.routes().capabilities() {
            if self
                .scene
                .capability
                .get(route.capability_uri())
                .and_then(Option::as_ref)
                .is_none()
            {
                return Err(missing_scene_factory(
                    RenderRouteKind::Capability,
                    route.capability_uri(),
                    route.identity(),
                ));
            }
        }
        Ok(ActivatedRenderWorldContract {
            payload: self,
            document_composer: composer,
        })
    }
}

impl std::ops::Deref for ActivatedRenderPayloadContract {
    type Target = RenderWorldContract;
    fn deref(&self) -> &Self::Target {
        &self.executable
    }
}

/// Exact payload activation with every mandatory scene factory admitted.
pub struct ActivatedRenderWorldContract {
    payload: ActivatedRenderPayloadContract,
    document_composer: Arc<dyn SceneCompositionFactory>,
}

impl ActivatedRenderWorldContract {
    /// Returns the exact validated document used during activation.
    #[must_use]
    pub const fn authored_contract(&self) -> &ContractDocument {
        self.payload.authored_contract()
    }

    pub(crate) fn document_scene_composer(&self) -> &Arc<dyn SceneCompositionFactory> {
        &self.document_composer
    }

    pub(crate) fn component_scene_factory(
        &self,
        schema_uri: &str,
    ) -> Option<&Arc<dyn RouteSceneFactory>> {
        self.payload
            .scene
            .component
            .get(schema_uri)
            .and_then(Option::as_ref)
    }

    pub(crate) fn capability_scene_factory(
        &self,
        capability_uri: &str,
    ) -> Option<&Arc<dyn RouteSceneFactory>> {
        self.payload
            .scene
            .capability
            .get(capability_uri)
            .and_then(Option::as_ref)
    }
}

impl std::ops::Deref for ActivatedRenderWorldContract {
    type Target = ActivatedRenderPayloadContract;
    fn deref(&self) -> &Self::Target {
        &self.payload
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
    activate_contract(document, registry, SceneRequirement::Required)?.into_render_contract()
}

/// Binds exact authored handlers and adapters without requiring scene factories.
///
/// # Errors
/// Returns the same document, factory, descriptor and adapter errors as full
/// activation. Scene-factory presence is checked only when admitting rendering.
pub fn activate_render_payload_contract(
    document: &ContractDocument,
    registry: &RenderImplementationRegistry,
) -> Result<ActivatedRenderPayloadContract, RenderImplementationError> {
    activate_contract(document, registry, SceneRequirement::Deferred)
}

#[derive(Clone, Copy)]
enum SceneRequirement {
    Deferred,
    Required,
}

impl SceneRequirement {
    fn admit<F: ?Sized>(
        self,
        factory: Option<Arc<F>>,
        kind: RenderRouteKind,
        key: &str,
        identity: &RouteIdentity,
    ) -> Result<Option<Arc<F>>, RenderImplementationError> {
        if matches!(self, Self::Required) && factory.is_none() {
            return Err(missing_scene_factory(kind, key, identity));
        }
        Ok(factory)
    }
}

fn activate_contract(
    document: &ContractDocument,
    registry: &RenderImplementationRegistry,
    scene_requirement: SceneRequirement,
) -> Result<ActivatedRenderPayloadContract, RenderImplementationError> {
    document.validate()?;
    let mut builder = RenderWorldContractBuilder::new();
    let mut component_adapters = BTreeMap::new();
    let mut resource_adapters = BTreeMap::new();
    let mut component_scene = BTreeMap::new();
    let mut capability_scene = BTreeMap::new();
    let document_composer = activate_document_route(
        document.routes().document(),
        registry,
        &mut builder,
        scene_requirement,
    )?;
    for route in document.routes().components() {
        let factory = activate_component_route(
            route,
            registry,
            &mut builder,
            &mut component_adapters,
            scene_requirement,
        )?;
        component_scene.insert(route.schema_uri().to_owned(), factory);
    }
    for route in document.routes().capabilities() {
        let factory = activate_capability_route(
            route,
            registry,
            &mut builder,
            &mut resource_adapters,
            scene_requirement,
        )?;
        capability_scene.insert(route.capability_uri().to_owned(), factory);
    }
    let executable = builder.build().map_err(RenderImplementationError::from)?;
    Ok(ActivatedRenderPayloadContract {
        authored: document.clone(),
        executable,
        identity: Arc::new(()),
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
    scene_requirement: SceneRequirement,
) -> Result<Option<Arc<dyn SceneCompositionFactory>>, RenderImplementationError> {
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
    let composer = scene_requirement.admit(
        implementation.scene_composer().cloned(),
        RenderRouteKind::Document,
        route.schema_uri(),
        identity,
    )?;
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
    scene_requirement: SceneRequirement,
) -> Result<Option<Arc<dyn RouteSceneFactory>>, RenderImplementationError> {
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
    let scene_factory = scene_requirement.admit(
        implementation.scene_factory().cloned(),
        RenderRouteKind::Component,
        route.schema_uri(),
        identity,
    )?;
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
    scene_requirement: SceneRequirement,
) -> Result<Option<Arc<dyn RouteSceneFactory>>, RenderImplementationError> {
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
    let scene_factory = scene_requirement.admit(
        implementation.scene_factory().cloned(),
        RenderRouteKind::Capability,
        route.capability_uri(),
        identity,
    )?;
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
