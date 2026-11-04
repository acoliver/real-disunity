//! Immutable owned render world plans minted from verified normal publications.

#[cfg(test)]
mod tests;

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use worldrunner_world_model::{
    ContentDigest, GeneratedStateDigest, SchemaDescriptor, SemanticWorldDocument, SourceIdentity,
};
use worldrunner_world_runtime::{
    ContentAddress, ContributionKind, HandlerDescriptor, PreparedContribution,
    PreparedSemanticWorld, RuntimePlanner, VerifiedNormalPublication, VerifiedPublication,
};

use crate::adapter::{RenderPayload, RenderPayloadAdapter};
use crate::contract::RenderWorldContract;
use crate::discovery::{DiscoveredRenderContract, RENDER_CONTRACT_CAPABILITY_URI};
use crate::document::ContractDocument;
use crate::error::RenderPlanError;
use crate::implementation_registry::{
    ActivatedRenderPayloadContract, ActivatedRenderWorldContract, RenderRouteKind,
};
use crate::limits::RenderWorldLimits;
use crate::plan_closure::validate_authored_plan_closure;
use crate::plan_scene::compose_scene;
use crate::scene::RenderScene;

mod adaptation;
use adaptation::{
    AdapterUse, adapt_contributions, adapt_root, enforce_contribution_limits, parse_digest,
    verify_adapter_use,
};

/// One adapted contribution retained by a [`RenderWorldPlan`].
///
/// The contribution is fully owned CPU state: kind, claimant, payload
/// contract, producing handler identity, and the adapted payload all outlive
/// the publication, provider, and contract that produced them.
#[derive(Clone, Debug)]
pub struct RenderContribution {
    kind: ContributionKind,
    claimant: String,
    payload_contract_uri: String,
    handler: HandlerDescriptor,
    route_key: String,
    payload: RenderPayload,
}

impl RenderContribution {
    /// Adopts the fully verified contribution parts.
    pub(crate) fn new(
        kind: ContributionKind,
        claimant: String,
        payload_contract_uri: String,
        handler: HandlerDescriptor,
        route_key: String,
        payload: RenderPayload,
    ) -> Self {
        Self {
            kind,
            claimant,
            payload_contract_uri,
            handler,
            route_key,
            payload,
        }
    }

    /// Returns whether this contribution came from a component or a binding.
    #[must_use]
    pub fn kind(&self) -> ContributionKind {
        self.kind
    }

    /// Returns the component or binding identifier that produced this contribution.
    #[must_use]
    pub fn claimant(&self) -> &str {
        &self.claimant
    }

    /// Returns the exact payload contract the producing handler emitted.
    #[must_use]
    pub fn payload_contract_uri(&self) -> &str {
        &self.payload_contract_uri
    }

    /// Returns the descriptor of the handler that produced this contribution.
    #[must_use]
    pub fn handler(&self) -> &HandlerDescriptor {
        &self.handler
    }

    /// Returns the exact component schema or capability URI that routed this payload.
    #[must_use]
    pub fn route_key(&self) -> &str {
        &self.route_key
    }

    /// Returns the fully owned adapted CPU payload.
    #[must_use]
    pub fn payload(&self) -> &RenderPayload {
        &self.payload
    }
}

#[derive(Debug)]
pub struct RenderWorldPlan {
    prepared: PreparedRenderWorldPayloads,
    scene: RenderScene,
}

impl RenderWorldPlan {
    /// Returns the exact validated authored contract.
    #[must_use]
    pub const fn authored_contract(&self) -> &ContractDocument {
        self.prepared.authored_contract()
    }

    /// Returns the exact discovery binding, or None for a low-level build.
    #[must_use]
    pub fn governed_contract_binding_id(&self) -> Option<&str> {
        self.prepared.governed_contract_binding_id()
    }

    /// Returns the governed discovery content address.
    #[must_use]
    pub const fn governed_contract_address(&self) -> Option<ContentAddress> {
        self.prepared.governed_contract_address()
    }

    /// Returns the exact negotiated schema.
    #[must_use]
    pub fn schema(&self) -> &SchemaDescriptor {
        self.prepared.schema()
    }

    /// Returns the verified source identity.
    #[must_use]
    pub fn source(&self) -> &SourceIdentity {
        self.prepared.source()
    }

    /// Returns the verified generated-state digest.
    #[must_use]
    pub fn generated_state_digest(&self) -> GeneratedStateDigest {
        self.prepared.generated_state_digest()
    }

    /// Returns the independently verified corpus digest.
    #[must_use]
    pub fn corpus_manifest_digest(&self) -> ContentDigest {
        self.prepared.corpus_manifest_digest()
    }

    /// Returns the verified scope-policy digest.
    #[must_use]
    pub fn scope_policy_digest(&self) -> ContentDigest {
        self.prepared.scope_policy_digest()
    }

    /// Returns the handler that proved document closure.
    #[must_use]
    pub fn document_handler(&self) -> &HandlerDescriptor {
        self.prepared.document_handler()
    }

    /// Returns the exact adapted root payload contract.
    #[must_use]
    pub fn root_payload_contract_uri(&self) -> &str {
        self.prepared.root_payload_contract_uri()
    }

    /// Returns the wholly owned adapted root payload.
    #[must_use]
    pub fn root_payload(&self) -> &RenderPayload {
        self.prepared.root_payload()
    }

    /// Returns adapted contributions sorted by kind, contract and claimant.
    #[must_use]
    pub fn contributions(&self) -> &[RenderContribution] {
        self.prepared.contributions()
    }

    /// Returns the deterministic negotiated handler revisions.
    #[must_use]
    pub fn handler_revisions(&self) -> &BTreeMap<String, u64> {
        self.prepared.handler_revisions()
    }

    /// Returns the mandatory validated scene composed through the bound factories.
    #[must_use]
    pub const fn scene(&self) -> &RenderScene {
        &self.scene
    }
}

impl std::ops::Deref for RenderWorldPlan {
    type Target = PreparedRenderWorldPayloads;
    fn deref(&self) -> &Self::Target {
        &self.prepared
    }
}

/// Owned payload preparation after exact authored closure and adaptation.
///
/// No scene factory has run. The result retains publication identities and an
/// activation identity, without retaining handlers, adapters or providers, so
/// composition cannot substitute another executable binding.
#[derive(Debug)]
pub struct PreparedRenderWorldPayloads {
    activation_identity: Arc<()>,
    authored_contract: ContractDocument,
    governed_contract_binding_id: Option<String>,
    governed_contract_address: Option<ContentAddress>,
    schema: SchemaDescriptor,
    source: SourceIdentity,
    generated_state_digest: GeneratedStateDigest,
    corpus_manifest_digest: ContentDigest,
    scope_policy_digest: ContentDigest,
    document_handler: HandlerDescriptor,
    root_payload_contract_uri: String,
    root_payload: RenderPayload,
    contributions: Box<[RenderContribution]>,
    handler_revisions: BTreeMap<String, u64>,
}

impl PreparedRenderWorldPayloads {
    /// Returns the exact validated authored contract retained by this preparation.
    #[must_use]
    pub const fn authored_contract(&self) -> &ContractDocument {
        &self.authored_contract
    }

    /// Returns the selected-world binding that governed contract discovery.
    ///
    /// Direct low-level builds that deliberately receive a caller-supplied
    /// contract return `None`; discovery-aware builds always return `Some`.
    #[must_use]
    pub fn governed_contract_binding_id(&self) -> Option<&str> {
        self.governed_contract_binding_id.as_deref()
    }

    /// Returns the content address that governed contract discovery.
    #[must_use]
    pub const fn governed_contract_address(&self) -> Option<ContentAddress> {
        self.governed_contract_address
    }

    /// Returns the exact schema the world was negotiated under.
    #[must_use]
    pub fn schema(&self) -> &SchemaDescriptor {
        &self.schema
    }

    /// Returns the exact identity of the source the world was derived from.
    #[must_use]
    pub fn source(&self) -> &SourceIdentity {
        &self.source
    }

    /// Returns the verified generated-state digest of the planned world.
    #[must_use]
    pub fn generated_state_digest(&self) -> GeneratedStateDigest {
        self.generated_state_digest
    }

    #[must_use]
    pub fn corpus_manifest_digest(&self) -> ContentDigest {
        self.corpus_manifest_digest
    }

    /// Returns the scope-policy digest the publication was verified under.
    #[must_use]
    pub fn scope_policy_digest(&self) -> ContentDigest {
        self.scope_policy_digest
    }

    /// Returns the descriptor of the document handler that proved root closure.
    #[must_use]
    pub fn document_handler(&self) -> &HandlerDescriptor {
        &self.document_handler
    }

    /// Returns the root payload contract the root payload was adapted through.
    #[must_use]
    pub fn root_payload_contract_uri(&self) -> &str {
        &self.root_payload_contract_uri
    }

    /// Returns the fully owned adapted root payload.
    #[must_use]
    pub fn root_payload(&self) -> &RenderPayload {
        &self.root_payload
    }

    /// Returns all contributions sorted ascending by kind, contract, claimant.
    #[must_use]
    pub fn contributions(&self) -> &[RenderContribution] {
        &self.contributions
    }

    /// Returns the deterministic revision of every handler that negotiated.
    #[must_use]
    pub fn handler_revisions(&self) -> &BTreeMap<String, u64> {
        &self.handler_revisions
    }

    /// Returns payloads for one exact declared route, in deterministic plan order.
    /// A declared idle route returns an empty iterator.
    ///
    /// # Errors
    /// Returns a named route mismatch when the route was never declared.
    pub fn payloads_for_route<'a>(
        &'a self,
        kind: ContributionKind,
        route_key: &'a str,
    ) -> Result<impl Iterator<Item = &'a RenderContribution>, RenderPlanError> {
        payloads_for_route(
            &self.authored_contract,
            &self.contributions,
            kind,
            route_key,
        )
    }
}

/// Mints one owned render world plan from an independently verified normal publication.
///
/// The publication's
/// world document is prepared through [`RuntimePlanner::prepare`] against the
/// contract's internal runtime registry with the publication itself as the
/// resource provider and the caller's runtime limits; the prepared root,
/// components, and resources are then closed over the contract's exact routes
/// and payload contracts before each payload is adapted into owned CPU state.
///
/// # Errors
///
/// Returns [`RenderPlanError`] for the first fail-closed refusal: runtime
/// preparation failure, contribution count limits, unknown claimants, missing
/// routes, wrong producing handlers or payload contracts, missing or unused
/// adapters, adapter rejections, root handler drift, or malformed publication
/// digests.
pub fn build_render_world_plan(
    normal: &VerifiedNormalPublication,
    activated: &ActivatedRenderWorldContract,
    limits: &RenderWorldLimits,
) -> Result<RenderWorldPlan, RenderPlanError> {
    build_render_world_plan_inner(normal.publication(), activated, limits, None)
}

/// Mints one render plan whose authored document is bound to governed discovery.
///
/// # Errors
///
/// Returns [`RenderPlanError::GovernedContractSubstitution`] if discovery does
/// not match the publication's unique bare render-contract binding and content
/// address, or activation used a different authored document.
pub fn build_discovered_render_world_plan(
    normal: &VerifiedNormalPublication,
    discovered: &DiscoveredRenderContract,
    activated: &ActivatedRenderWorldContract,
    limits: &RenderWorldLimits,
) -> Result<RenderWorldPlan, RenderPlanError> {
    validate_discovered_binding(normal, discovered, activated.authored_contract())?;
    build_render_world_plan_inner(
        normal.publication(),
        activated,
        limits,
        Some((discovered.binding_id(), discovered.address())),
    )
}

/// Prepares owned CPU payloads without invoking any scene factory.
///
/// # Errors
/// Returns the same closure, runtime, limit and adaptation failures as a full build.
pub fn prepare_render_world_payloads(
    normal: &VerifiedNormalPublication,
    activated: &ActivatedRenderPayloadContract,
    limits: &RenderWorldLimits,
) -> Result<PreparedRenderWorldPayloads, RenderPlanError> {
    prepare_render_world_payloads_inner(normal.publication(), activated, limits, None)
}

/// Prepares payloads while retaining the exact governed discovery binding.
///
/// # Errors
/// Returns [`RenderPlanError::GovernedContractSubstitution`] if discovery does
/// not match the publication's unique bare render-contract binding and content
/// address or the activated authored document; otherwise returns preparation failures.
pub fn prepare_discovered_render_world_payloads(
    normal: &VerifiedNormalPublication,
    discovered: &DiscoveredRenderContract,
    activated: &ActivatedRenderPayloadContract,
    limits: &RenderWorldLimits,
) -> Result<PreparedRenderWorldPayloads, RenderPlanError> {
    validate_discovered_binding(normal, discovered, activated.authored_contract())?;
    prepare_render_world_payloads_inner(
        normal.publication(),
        activated,
        limits,
        Some((discovered.binding_id(), discovered.address())),
    )
}

/// Composes a prepared result through the same activation's mandatory scene factories.
///
/// # Errors
/// Returns a named executable-contract mismatch for a different activation,
/// or the existing factory, composition and scene validation errors.
pub fn compose_render_world_plan(
    prepared: PreparedRenderWorldPayloads,
    activated: &ActivatedRenderWorldContract,
) -> Result<RenderWorldPlan, RenderPlanError> {
    if !Arc::ptr_eq(
        &prepared.activation_identity,
        activated.activation_identity(),
    ) {
        return Err(RenderPlanError::AuthoredRouteMismatch {
            route_kind: RenderRouteKind::Document,
            route_key: prepared
                .authored_contract
                .routes()
                .document()
                .schema_uri()
                .to_owned(),
            field: "prepared executable contract",
        });
    }
    let scene = compose_scene(activated, &prepared.root_payload, &prepared.contributions)?;
    Ok(RenderWorldPlan { prepared, scene })
}

fn validate_discovered_binding(
    normal: &VerifiedNormalPublication,
    discovered: &DiscoveredRenderContract,
    authored: &ContractDocument,
) -> Result<(), RenderPlanError> {
    let mut matches = normal
        .publication()
        .world()
        .bindings()
        .iter()
        .filter(|binding| binding.capability_uri == RENDER_CONTRACT_CAPABILITY_URI);
    let Some(binding) = matches.next() else {
        return Err(RenderPlanError::GovernedContractSubstitution);
    };
    if matches.next().is_some()
        || binding.element_uri.is_some()
        || binding.binding_id != discovered.binding_id()
        || ContentAddress::from(binding) != discovered.address()
        || discovered.document() != authored
    {
        return Err(RenderPlanError::GovernedContractSubstitution);
    }
    Ok(())
}

fn build_render_world_plan_inner(
    publication: &VerifiedPublication,
    activated: &ActivatedRenderWorldContract,
    limits: &RenderWorldLimits,
    governed_contract: Option<(&str, ContentAddress)>,
) -> Result<RenderWorldPlan, RenderPlanError> {
    let prepared =
        prepare_render_world_payloads_inner(publication, activated, limits, governed_contract)?;
    compose_render_world_plan(prepared, activated)
}

fn prepare_render_world_payloads_inner(
    publication: &VerifiedPublication,
    activated: &ActivatedRenderPayloadContract,
    limits: &RenderWorldLimits,
    governed_contract: Option<(&str, ContentAddress)>,
) -> Result<PreparedRenderWorldPayloads, RenderPlanError> {
    let authored_contract = activated.authored_contract();
    let contract = activated.executable_contract();
    validate_authored_plan_closure(publication.world(), authored_contract, contract)?;
    let prepared = RuntimePlanner::new(contract.runtime_registry()).prepare(
        publication.world(),
        limits.runtime(),
        publication,
    )?;
    enforce_contribution_limits(&prepared, limits)?;
    let mut used_adapters = AdapterUse::default();
    let contributions =
        adapt_contributions(publication.world(), contract, &prepared, &mut used_adapters)?;
    verify_adapter_use(contract, &used_adapters)?;
    let root_payload = adapt_root(contract, &prepared)?;
    let corpus_manifest_digest = parse_digest(
        "corpus_manifest_digest",
        &publication.receipt().corpus_manifest_digest,
    )?;
    let scope_policy_digest = parse_digest(
        "scope_policy_digest",
        &publication.receipt().scope_policy_digest,
    )?;
    Ok(PreparedRenderWorldPayloads {
        activation_identity: Arc::clone(activated.activation_identity()),
        authored_contract: authored_contract.clone(),
        governed_contract_binding_id: governed_contract
            .map(|(binding_id, _)| binding_id.to_owned()),
        governed_contract_address: governed_contract.map(|(_, address)| address),
        schema: prepared.schema().clone(),
        source: *prepared.source(),
        generated_state_digest: publication.generated_state_digest(),
        corpus_manifest_digest,
        scope_policy_digest,
        document_handler: prepared.root_handler().clone(),
        root_payload_contract_uri: contract.root_payload_contract_uri().to_owned(),
        root_payload,
        contributions: contributions.into_boxed_slice(),
        handler_revisions: prepared.handler_revisions().clone(),
    })
}

/// Resolves a declared route independently of payload contract sharing.
pub(crate) fn payloads_for_route<'a>(
    authored: &ContractDocument,
    contributions: &'a [RenderContribution],
    kind: ContributionKind,
    route_key: &'a str,
) -> Result<impl Iterator<Item = &'a RenderContribution>, RenderPlanError> {
    let (declared, route_kind) = match kind {
        ContributionKind::Component => (
            authored
                .routes()
                .components()
                .iter()
                .any(|route| route.schema_uri() == route_key),
            RenderRouteKind::Component,
        ),
        ContributionKind::Resource => (
            authored
                .routes()
                .capabilities()
                .iter()
                .any(|route| route.capability_uri() == route_key),
            RenderRouteKind::Capability,
        ),
    };
    if !declared {
        return Err(RenderPlanError::AuthoredRouteMismatch {
            route_kind,
            route_key: route_key.to_owned(),
            field: "prepared payload route",
        });
    }
    Ok(contributions
        .iter()
        .filter(move |entry| entry.kind() == kind && entry.route_key() == route_key))
}
