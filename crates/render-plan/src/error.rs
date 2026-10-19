//! Typed fail-closed failures for render plan minting.

use thiserror::Error;
use worldrunner_world_model::{DigestError, WorldModelError};
use worldrunner_world_runtime::{
    ContributionKind, HandlerDescriptor, RuntimePlanError, RuntimeRegistryError,
};

use crate::contract::ContractRegistration;
use crate::document::ContractDocumentError;
use crate::implementation_registry::RenderRouteKind;
use crate::scene::SceneValidationError;

/// Failures raised while building a [`crate::RenderWorldContract`].
#[derive(Debug, Error)]
pub enum RenderContractError {
    /// Two registrations collided on one exact contract key.
    #[error("duplicate {kind:?} registration for {key:?}")]
    DuplicateRegistration {
        /// The kind of registration that collided.
        kind: ContractRegistration,
        /// The exact key that already carried a registration.
        key: String,
    },
    /// The builder finished without one mandatory registration.
    #[error("exactly one {kind:?} must be registered before build")]
    MissingRegistration {
        /// The kind of registration that was absent.
        kind: ContractRegistration,
    },
    /// A registration key violated the world model's identifier rules.
    #[error("invalid {kind:?} key {key:?}: {source}")]
    InvalidKey {
        /// The kind of registration carrying the malformed key.
        kind: ContractRegistration,
        /// The rejected key.
        key: String,
        /// Universal identifier validation failure.
        source: WorldModelError,
    },
    /// A route expected a payload contract with no registered adapter.
    #[error(
        "{route_kind:?} route {route_key:?} expects payload contract {payload_contract_uri:?} with no registered {adapter_kind:?} adapter"
    )]
    UnresolvedAdapter {
        /// The kind of route carrying the expectation.
        route_kind: ContractRegistration,
        /// The routing key whose expectation is unresolved.
        route_key: String,
        /// The kind of adapter registration that is missing.
        adapter_kind: ContractRegistration,
        /// The exact expected payload-contract URI.
        payload_contract_uri: String,
    },
    /// A registered adapter is referenced by no route and can never be used.
    #[error(
        "registered {kind:?} adapter for payload contract {payload_contract_uri:?} is declared by no route"
    )]
    OrphanAdapter {
        /// The contribution kind the adapter serves.
        kind: ContributionKind,
        /// The payload-contract URI nothing routes to.
        payload_contract_uri: String,
    },
    /// The internal runtime registry rejected a registration.
    #[error(transparent)]
    Registry(#[from] RuntimeRegistryError),
}

/// Typed failures raised while registering implementation factories or
/// activating one authored document against a
/// [`crate::RenderImplementationRegistry`].
#[derive(Debug, Error)]
pub enum RenderImplementationError {
    /// The document violated an authored invariant before any lookup.
    #[error("authored contract document is malformed: {source}")]
    Document {
        /// The authored-document validation failure.
        source: ContractDocumentError,
    },
    /// A registration named a malformed factory URI.
    #[error("invalid {route_kind:?} factory URI {handler_factory_uri:?}: {source}")]
    InvalidFactoryUri {
        /// The route kind the registration targeted.
        route_kind: RenderRouteKind,
        /// The rejected factory URI.
        handler_factory_uri: String,
        /// Universal URI validation failure.
        source: WorldModelError,
    },
    /// Two registrations of one route kind collided on one factory URI.
    #[error("duplicate {route_kind:?} factory registration for {handler_factory_uri:?}")]
    DuplicateFactory {
        /// The route kind carrying the collision.
        route_kind: RenderRouteKind,
        /// The exact factory URI that already carried a registration.
        handler_factory_uri: String,
    },
    /// A present declaration named a factory URI the registry never
    /// registered for that exact route kind.
    #[error(
        "no {route_kind:?} factory registered for {handler_factory_uri:?} required by route {route_key:?}"
    )]
    UnknownFactory {
        /// The route kind whose registry lookup failed.
        route_kind: RenderRouteKind,
        /// The schema or capability URI of the declaration.
        route_key: String,
        /// The exact unregistered factory URI.
        handler_factory_uri: String,
    },
    /// A factory produced a handler descriptor that does not exactly match
    /// the authored handler identity and revision.
    #[error(
        "{route_kind:?} factory {handler_factory_uri:?} for route {route_key:?} produced handler \
         {observed:?} but the declaration authored {expected_handler_id:?} revision \
         {expected_handler_revision}"
    )]
    HandlerMismatch {
        /// The route kind whose factory drifted.
        route_kind: RenderRouteKind,
        /// The schema or capability URI of the declaration.
        route_key: String,
        /// The factory URI that produced the drifted handler.
        handler_factory_uri: String,
        /// The authored handler identifier.
        expected_handler_id: String,
        /// The authored handler revision.
        expected_handler_revision: u64,
        /// The descriptor the factory actually produced.
        observed: HandlerDescriptor,
    },
    /// A factory refused one exact declaration.
    #[error(
        "{route_kind:?} factory {handler_factory_uri:?} for route {route_key:?} rejected the \
         declaration: {reason:?}"
    )]
    FactoryRejected {
        /// The route kind whose factory refused.
        route_kind: RenderRouteKind,
        /// The schema or capability URI of the declaration.
        route_key: String,
        /// The factory URI that refused.
        handler_factory_uri: String,
        /// The factory's typed refusal reason.
        reason: String,
    },
    /// Two routes declared one payload contract but their factories produced
    /// distinct adapters, leaving activation no exact adapter to bind.
    #[error(
        "{route_kind:?} routes {route_key:?} and an earlier route declare payload contract \
         {payload_contract_uri:?} whose factories produced distinct adapters"
    )]
    ConflictingAdapter {
        /// The route kind carrying the shared payload contract.
        route_kind: RenderRouteKind,
        /// The schema or capability URI of the later declaration.
        route_key: String,
        /// The payload contract URI two adapters claimed.
        payload_contract_uri: String,
    },
    /// An implementation factory produced no scene factory for its route, so
    /// activation cannot bind the mandatory scene phase.
    #[error(
        "{route_kind:?} factory {handler_factory_uri:?} for route {route_key:?} produced no \
         scene factory"
    )]
    MissingSceneFactory {
        /// The route kind whose implementation lacks a scene factory.
        route_kind: RenderRouteKind,
        /// The schema or capability URI of the declaration.
        route_key: String,
        /// The exact handler factory URI that omitted the scene factory.
        handler_factory_uri: String,
    },
    /// The underlying contract builder rejected a wired registration.
    #[error("render world contract construction failed: {source}")]
    Contract {
        /// The contract-builder failure.
        source: RenderContractError,
    },
}

impl From<ContractDocumentError> for RenderImplementationError {
    fn from(source: ContractDocumentError) -> Self {
        Self::Document { source }
    }
}

impl From<RenderContractError> for RenderImplementationError {
    fn from(source: RenderContractError) -> Self {
        Self::Contract { source }
    }
}

/// Typed failures raised by one [`crate::RenderPayloadAdapter`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RenderAdapterError {
    /// The prepared payload carried a different Rust contract type.
    #[error("payload carries {observed:?} but the adapter requires {expected:?}")]
    WrongPayloadType {
        /// Type name the adapter requires.
        expected: &'static str,
        /// Type name the prepared payload carried.
        observed: &'static str,
    },
    /// The adapter rejected otherwise well-typed content.
    #[error("payload content rejected: {reason:?}")]
    Rejected {
        /// Description of the violated contract.
        reason: String,
    },
}

/// Failures that can occur while minting one [`crate::RenderWorldPlan`].
#[derive(Debug, Error)]
pub enum RenderPlanError {
    /// The authored document failed strict validation at plan-build time.
    #[error("authored contract document is malformed: {source}")]
    AuthoredDocument {
        /// The authored-document validation failure.
        source: ContractDocumentError,
    },
    /// Activation used a document other than the content-addressed discovered authority.
    #[error("activated render contract differs from the governed discovered document")]
    GovernedContractSubstitution,
    /// The executable contract does not exactly implement one authored route field.
    #[error("activated {route_kind:?} route {route_key:?} differs from the authored {field}")]
    AuthoredRouteMismatch {
        /// The route kind whose executable registration drifted.
        route_kind: RenderRouteKind,
        /// The exact authored route key.
        route_key: String,
        /// The exact identity field that drifted.
        field: &'static str,
    },
    /// A publication population cannot be represented by the authored u64 contract.
    #[error("{route_kind:?} route {route_key:?} population cannot be represented as u64")]
    RoutePopulationOverflow {
        /// The route kind whose population overflowed.
        route_kind: RenderRouteKind,
        /// The exact schema or capability URI.
        route_key: String,
    },
    /// One authored route's population lies outside its exact cardinality bounds.
    #[error(
        "{route_kind:?} route {route_key:?} population {observed} is outside authored bounds {minimum}..={maximum}"
    )]
    RouteCardinalityViolation {
        /// The route kind whose population was counted.
        route_kind: RenderRouteKind,
        /// The exact schema or capability URI.
        route_key: String,
        /// The authored inclusive minimum.
        minimum: u64,
        /// The authored inclusive maximum.
        maximum: u64,
        /// The exact publication population.
        observed: u64,
    },
    /// The authored asset-binding set and publication resource-binding set differ.
    #[error("asset binding {binding_id:?} {reason}")]
    AssetBindingClosureMismatch {
        /// The exact logical binding identifier.
        binding_id: String,
        /// Which side of the exact closure omitted the binding.
        reason: &'static str,
    },
    /// A resource binding violates its capability route's authored element policy.
    #[error(
        "resource binding {binding_id:?} under capability {capability_uri:?} violates the authored element policy"
    )]
    BindingElementPolicyMismatch {
        /// The exact logical binding identifier.
        binding_id: String,
        /// The exact capability URI routing the binding.
        capability_uri: String,
    },
    /// A resource binding's authored asset role differs from its capability-route role.
    #[error(
        "asset binding {binding_id:?} carries role {asset_role_uri:?} but capability {capability_uri:?} declares role {route_role_uri:?}"
    )]
    AssetRoleMismatch {
        /// The exact logical binding identifier.
        binding_id: String,
        /// The role authored for this exact asset binding.
        asset_role_uri: String,
        /// The binding's exact capability URI.
        capability_uri: String,
        /// The role authored by the capability route.
        route_role_uri: String,
    },
    /// Runtime preparation of the publication's world failed.
    #[error("runtime preparation failed: {source}")]
    Runtime {
        /// The underlying fail-closed runtime refusal.
        source: RuntimePlanError,
    },
    /// The prepared world carries more component contributions than permitted.
    #[error(
        "prepared world carries {observed} component contributions but the caller permits at most {max}"
    )]
    ComponentContributionLimitExceeded {
        /// The caller's maximum prepared component count.
        max: usize,
        /// The observed prepared component count.
        observed: usize,
    },
    /// The prepared world carries more resource contributions than permitted.
    #[error(
        "prepared world carries {observed} resource contributions but the caller permits at most {max}"
    )]
    ResourceContributionLimitExceeded {
        /// The caller's maximum prepared resource count.
        max: usize,
        /// The observed prepared resource count.
        observed: usize,
    },
    /// A prepared contribution's claimant is absent from the verified world.
    #[error("{kind:?} contribution claimant {claimant:?} is absent from the verified world")]
    UnknownClaimant {
        /// The contribution kind whose claimant is unknown.
        kind: ContributionKind,
        /// The claimant the world does not carry.
        claimant: String,
    },
    /// A world component or binding routes to no contract route.
    #[error("no route for {kind:?} key {route_key:?} of claimant {claimant:?}")]
    MissingRoute {
        /// The contribution kind that failed to route.
        kind: ContributionKind,
        /// The schema or capability URI that carries no route.
        route_key: String,
        /// The claimant whose routing failed.
        claimant: String,
    },
    /// A producing handler emitted a different payload-contract URI than its route requires.
    #[error(
        "{kind:?} contribution of {claimant:?} under {route_key:?} carries payload contract {observed:?} but the route requires {expected:?}"
    )]
    WrongContributionContract {
        /// The contribution kind that mismatched.
        kind: ContributionKind,
        /// The schema or capability URI routing the claimant.
        route_key: String,
        /// The claimant whose contribution mismatched.
        claimant: String,
        /// The handler that produced the contribution.
        handler: Box<HandlerDescriptor>,
        /// The payload-contract URI the route requires.
        expected: String,
        /// The payload-contract URI the handler emitted.
        observed: String,
    },
    /// A prepared contribution reports a handler the route never registered.
    #[error(
        "{kind:?} contribution of {claimant:?} under {route_key:?} reports handler {observed:?} but the route registered {expected:?}"
    )]
    WrongProducingHandler {
        /// The contribution kind that mismatched.
        kind: ContributionKind,
        /// The schema or capability URI routing the claimant.
        route_key: String,
        /// The claimant whose producing handler mismatched.
        claimant: String,
        /// The handler descriptor the route registered.
        expected: HandlerDescriptor,
        /// The handler descriptor the contribution reported.
        observed: HandlerDescriptor,
    },
    /// No adapter is registered for one exact contribution payload contract.
    #[error(
        "no adapter for {kind:?} payload contract {payload_contract_uri:?} required by {claimant:?}"
    )]
    MissingAdapter {
        /// The contribution kind lacking an adapter.
        kind: ContributionKind,
        /// The payload-contract URI lacking an adapter.
        payload_contract_uri: String,
        /// The claimant requiring the adapter.
        claimant: String,
    },
    /// An adapter rejected one contribution's prepared payload.
    #[error(
        "adapter for {kind:?} payload contract {payload_contract_uri:?} rejected contribution of {claimant:?}: {source}"
    )]
    AdapterRejected {
        /// The contribution kind whose payload was rejected.
        kind: ContributionKind,
        /// The payload-contract URI whose adapter rejected.
        payload_contract_uri: String,
        /// The claimant whose payload was rejected.
        claimant: String,
        /// The adapter's typed failure.
        source: RenderAdapterError,
    },
    /// A configured adapter stayed unused and no declared route permits that absence.
    #[error(
        "adapter for {kind:?} payload contract {payload_contract_uri:?} is configured but unused and not declared by any route"
    )]
    UnusedAdapter {
        /// The contribution kind whose adapter stayed unused.
        kind: ContributionKind,
        /// The payload-contract URI whose adapter stayed unused.
        payload_contract_uri: String,
    },
    /// The prepared root reports a handler the contract never registered.
    #[error("document root reports handler {observed:?} but the contract registered {expected:?}")]
    RootHandlerMismatch {
        /// The handler descriptor the contract registered.
        expected: HandlerDescriptor,
        /// The handler descriptor the prepared root reported.
        observed: HandlerDescriptor,
    },
    /// The root adapter rejected the prepared document payload.
    #[error(
        "root adapter for payload contract {payload_contract_uri:?} rejected the document payload: {source}"
    )]
    RootAdapterRejected {
        /// Exact external contract assigned to the root payload.
        payload_contract_uri: String,
        /// The adapter's typed failure.
        source: RenderAdapterError,
    },
    /// The verified publication carries a digest this crate cannot parse.
    #[error("verified publication carries a malformed {field} digest: {source}")]
    MalformedDigest {
        /// The receipt field carrying the malformed digest.
        field: &'static str,
        /// The strict digest parsing failure.
        source: DigestError,
    },
    /// An activation-bound route scene factory refused one exact contribution.
    #[error(
        "{route_kind:?} scene factory {handler_factory_uri:?} for route {route_key:?} refused \
         the contribution of {claimant:?}: {reason:?}"
    )]
    SceneFactoryRejected {
        /// The route kind whose scene factory refused.
        route_kind: RenderRouteKind,
        /// The schema or capability URI routing the claimant.
        route_key: String,
        /// The exact handler factory URI that bound the scene factory.
        handler_factory_uri: String,
        /// The claimant whose contribution was refused.
        claimant: String,
        /// The scene factory's typed refusal reason.
        reason: String,
    },
    /// The activation-bound document scene composition factory refused the
    /// complete composed scene.
    #[error(
        "document scene composition factory {handler_factory_uri:?} refused the composed \
         scene: {reason:?}"
    )]
    SceneCompositionRejected {
        /// The exact handler factory URI that bound the composer.
        handler_factory_uri: String,
        /// The composer's typed refusal reason.
        reason: String,
    },
    /// A composed scene violated its own internal closure.
    #[error("composed scene failed validation: {source}")]
    SceneValidation {
        /// The scene closure failure.
        source: SceneValidationError,
    },
}

impl From<RuntimePlanError> for RenderPlanError {
    fn from(source: RuntimePlanError) -> Self {
        Self::Runtime { source }
    }
}
