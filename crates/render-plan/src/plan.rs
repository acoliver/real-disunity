//! Immutable owned render world plans minted from verified normal publications.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use worldrunner_world_model::{
    ContentDigest, GeneratedStateDigest, SchemaDescriptor, SemanticWorldDocument, SourceIdentity,
};
use worldrunner_world_runtime::{
    ContentAddress, ContributionKind, HandlerDescriptor, PreparedContribution,
    PreparedSemanticWorld, RuntimePlanner, VerifiedNormalPublication,
};

use crate::adapter::{RenderPayload, RenderPayloadAdapter};
use crate::contract::RenderWorldContract;
use crate::discovery::DiscoveredRenderContract;
use crate::document::ContractDocument;
use crate::error::RenderPlanError;
use crate::implementation_registry::ActivatedRenderWorldContract;
use crate::limits::RenderWorldLimits;
use crate::plan_closure::validate_authored_plan_closure;
use crate::plan_scene::compose_scene;
use crate::scene::RenderScene;

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
    payload: RenderPayload,
}

impl RenderContribution {
    /// Adopts the fully verified contribution parts.
    pub(crate) fn new(
        kind: ContributionKind,
        claimant: String,
        payload_contract_uri: String,
        handler: HandlerDescriptor,
        payload: RenderPayload,
    ) -> Self {
        Self {
            kind,
            claimant,
            payload_contract_uri,
            handler,
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

    /// Returns the fully owned adapted CPU payload.
    #[must_use]
    pub fn payload(&self) -> &RenderPayload {
        &self.payload
    }
}

#[derive(Debug)]
pub struct RenderWorldPlan {
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
    scene: RenderScene,
}

impl RenderWorldPlan {
    /// Returns the exact validated authored contract retained by this plan.
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

    /// Returns the mandatory composed renderer-domain scene of this plan.
    ///
    /// The scene is wholly owned CPU state composed once through the
    /// activation-bound scene factories; no public API can replace or
    /// recompose it after the plan is minted.
    #[must_use]
    pub const fn scene(&self) -> &RenderScene {
        &self.scene
    }
}

/// Mints one owned render world plan from an independently verified normal publication.
///
/// This is the only way to obtain a [`RenderWorldPlan`]. The publication's
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
    build_render_world_plan_inner(normal, activated, limits, None)
}

/// Mints one render plan whose authored document is bound to governed discovery.
///
/// # Errors
///
/// Returns [`RenderPlanError::GovernedContractSubstitution`] if activation used
/// any document other than the exact decoded content-addressed authority.
pub fn build_discovered_render_world_plan(
    normal: &VerifiedNormalPublication,
    discovered: &DiscoveredRenderContract,
    activated: &ActivatedRenderWorldContract,
    limits: &RenderWorldLimits,
) -> Result<RenderWorldPlan, RenderPlanError> {
    if discovered.document() != activated.authored_contract() {
        return Err(RenderPlanError::GovernedContractSubstitution);
    }
    build_render_world_plan_inner(
        normal,
        activated,
        limits,
        Some((discovered.binding_id(), discovered.address())),
    )
}

fn build_render_world_plan_inner(
    normal: &VerifiedNormalPublication,
    activated: &ActivatedRenderWorldContract,
    limits: &RenderWorldLimits,
    governed_contract: Option<(&str, ContentAddress)>,
) -> Result<RenderWorldPlan, RenderPlanError> {
    let publication = normal.publication();
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
    let scene = compose_scene(
        publication.world(),
        activated,
        &root_payload,
        &contributions,
    )?;
    Ok(RenderWorldPlan {
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
        scene,
    })
}

/// Records which payload contracts each adapter kind actually consumed.
#[derive(Default)]
struct AdapterUse {
    component: BTreeSet<String>,
    resource: BTreeSet<String>,
}

impl AdapterUse {
    fn record(&mut self, kind: ContributionKind, payload_contract_uri: &str) {
        match kind {
            ContributionKind::Component => {
                self.component.insert(payload_contract_uri.to_owned());
            }
            ContributionKind::Resource => {
                self.resource.insert(payload_contract_uri.to_owned());
            }
        }
    }

    fn contains(&self, kind: ContributionKind, payload_contract_uri: &str) -> bool {
        match kind {
            ContributionKind::Component => self.component.contains(payload_contract_uri),
            ContributionKind::Resource => self.resource.contains(payload_contract_uri),
        }
    }
}

/// One contribution whose route closure passed, paired with its exact adapter.
struct ResolvedContribution<'a> {
    contribution: &'a PreparedContribution,
    adapter: &'a Arc<dyn RenderPayloadAdapter>,
}

fn enforce_contribution_limits(
    prepared: &PreparedSemanticWorld,
    limits: &RenderWorldLimits,
) -> Result<(), RenderPlanError> {
    let observed = prepared.component_contributions().len();
    if observed > limits.max_prepared_components() {
        return Err(RenderPlanError::ComponentContributionLimitExceeded {
            max: limits.max_prepared_components(),
            observed,
        });
    }
    let observed = prepared.resource_contributions().len();
    if observed > limits.max_prepared_resources() {
        return Err(RenderPlanError::ResourceContributionLimitExceeded {
            max: limits.max_prepared_resources(),
            observed,
        });
    }
    Ok(())
}

fn adapt_contributions(
    world: &SemanticWorldDocument,
    contract: &RenderWorldContract,
    prepared: &PreparedSemanticWorld,
    used: &mut AdapterUse,
) -> Result<Vec<RenderContribution>, RenderPlanError> {
    let mut resources = resolve_then_adapt(
        ContributionKind::Resource,
        prepared.resource_contributions(),
        world,
        contract,
        used,
    )?;
    let mut components = resolve_then_adapt(
        ContributionKind::Component,
        prepared.component_contributions(),
        world,
        contract,
        used,
    )?;
    components.append(&mut resources);
    // Ordering is an explicit plan invariant, not an accident of preparation.
    components.sort_by(contribution_order);
    Ok(components)
}

fn resolve_then_adapt(
    kind: ContributionKind,
    contributions: &[PreparedContribution],
    world: &SemanticWorldDocument,
    contract: &RenderWorldContract,
    used: &mut AdapterUse,
) -> Result<Vec<RenderContribution>, RenderPlanError> {
    let resolved = verify_route_closure(kind, contributions, world, contract, used)?;
    let mut adapted = Vec::with_capacity(resolved.len());
    for entry in resolved {
        let payload = entry
            .adapter
            .adapt(&entry.contribution.payload)
            .map_err(|source| RenderPlanError::AdapterRejected {
                kind,
                payload_contract_uri: entry.contribution.payload_contract_uri.clone(),
                claimant: entry.contribution.claimant.clone(),
                source,
            })?;
        adapted.push(RenderContribution::new(
            kind,
            entry.contribution.claimant.clone(),
            entry.contribution.payload_contract_uri.clone(),
            entry.contribution.handler.clone(),
            payload,
        ));
    }
    Ok(adapted)
}

fn verify_route_closure<'a>(
    kind: ContributionKind,
    contributions: &'a [PreparedContribution],
    world: &SemanticWorldDocument,
    contract: &'a RenderWorldContract,
    used: &mut AdapterUse,
) -> Result<Vec<ResolvedContribution<'a>>, RenderPlanError> {
    let mut resolved = Vec::with_capacity(contributions.len());
    for contribution in contributions {
        let route_key = claimant_route_key(kind, &contribution.claimant, world)?;
        verify_producing_handler(kind, &route_key, contribution, contract)?;
        used.record(kind, &contribution.payload_contract_uri);
        let adapter = resolve_adapter(kind, contribution, contract)?;
        resolved.push(ResolvedContribution {
            contribution,
            adapter,
        });
    }
    Ok(resolved)
}

pub(crate) fn claimant_route_key(
    kind: ContributionKind,
    claimant: &str,
    world: &SemanticWorldDocument,
) -> Result<String, RenderPlanError> {
    let components = world.components();
    let bindings = world.bindings();
    let route_key = match kind {
        ContributionKind::Component => components
            .binary_search_by(|component| component.component_id.as_str().cmp(claimant))
            .ok()
            .map(|index| components[index].schema_uri.clone()),
        ContributionKind::Resource => bindings
            .binary_search_by(|binding| binding.binding_id.as_str().cmp(claimant))
            .ok()
            .map(|index| bindings[index].capability_uri.clone()),
    };
    route_key.ok_or_else(|| RenderPlanError::UnknownClaimant {
        kind,
        claimant: claimant.to_owned(),
    })
}

fn verify_producing_handler(
    kind: ContributionKind,
    route_key: &str,
    contribution: &PreparedContribution,
    contract: &RenderWorldContract,
) -> Result<(), RenderPlanError> {
    let (registered, expected_uri) = match kind {
        ContributionKind::Component => {
            let route = contract.component_route(route_key).ok_or_else(|| {
                RenderPlanError::MissingRoute {
                    kind,
                    route_key: route_key.to_owned(),
                    claimant: contribution.claimant.clone(),
                }
            })?;
            (route.handler().descriptor(), route.payload_contract_uri())
        }
        ContributionKind::Resource => {
            let route = contract.capability_route(route_key).ok_or_else(|| {
                RenderPlanError::MissingRoute {
                    kind,
                    route_key: route_key.to_owned(),
                    claimant: contribution.claimant.clone(),
                }
            })?;
            (route.handler().descriptor(), route.payload_contract_uri())
        }
    };
    if &contribution.handler != registered {
        return Err(RenderPlanError::WrongProducingHandler {
            kind,
            route_key: route_key.to_owned(),
            claimant: contribution.claimant.clone(),
            expected: registered.clone(),
            observed: contribution.handler.clone(),
        });
    }
    if contribution.payload_contract_uri != expected_uri {
        return Err(RenderPlanError::WrongContributionContract {
            kind,
            route_key: route_key.to_owned(),
            claimant: contribution.claimant.clone(),
            handler: Box::new(contribution.handler.clone()),
            expected: expected_uri.to_owned(),
            observed: contribution.payload_contract_uri.clone(),
        });
    }
    Ok(())
}

fn resolve_adapter<'a>(
    kind: ContributionKind,
    contribution: &PreparedContribution,
    contract: &'a RenderWorldContract,
) -> Result<&'a Arc<dyn RenderPayloadAdapter>, RenderPlanError> {
    let adapter = match kind {
        ContributionKind::Component => {
            contract.component_adapter(&contribution.payload_contract_uri)
        }
        ContributionKind::Resource => contract.resource_adapter(&contribution.payload_contract_uri),
    };
    adapter.ok_or_else(|| RenderPlanError::MissingAdapter {
        kind,
        payload_contract_uri: contribution.payload_contract_uri.clone(),
        claimant: contribution.claimant.clone(),
    })
}

fn verify_adapter_use(
    contract: &RenderWorldContract,
    used: &AdapterUse,
) -> Result<(), RenderPlanError> {
    for payload_contract_uri in contract.component_adapter_uris() {
        require_adapter_used(
            ContributionKind::Component,
            payload_contract_uri,
            contract,
            used,
        )?;
    }
    for payload_contract_uri in contract.resource_adapter_uris() {
        require_adapter_used(
            ContributionKind::Resource,
            payload_contract_uri,
            contract,
            used,
        )?;
    }
    Ok(())
}

fn require_adapter_used(
    kind: ContributionKind,
    payload_contract_uri: &str,
    contract: &RenderWorldContract,
    used: &AdapterUse,
) -> Result<(), RenderPlanError> {
    if used.contains(kind, payload_contract_uri) {
        return Ok(());
    }
    if contract.is_route_scoped(kind, payload_contract_uri) {
        // A route-scoped adapter is declared for its route, not claimed
        // consumed: a world without matching instances leaves it honestly
        // idle, which is distinct from adapters nothing declared at all.
        return Ok(());
    }
    Err(RenderPlanError::UnusedAdapter {
        kind,
        payload_contract_uri: payload_contract_uri.to_owned(),
    })
}

fn adapt_root(
    contract: &RenderWorldContract,
    prepared: &PreparedSemanticWorld,
) -> Result<RenderPayload, RenderPlanError> {
    if prepared.root_handler() != contract.document_handler_descriptor() {
        return Err(RenderPlanError::RootHandlerMismatch {
            expected: contract.document_handler_descriptor().clone(),
            observed: prepared.root_handler().clone(),
        });
    }
    contract
        .root_adapter()
        .adapt(prepared.root_payload())
        .map_err(|source| RenderPlanError::RootAdapterRejected {
            payload_contract_uri: contract.root_payload_contract_uri().to_owned(),
            source,
        })
}

fn parse_digest(field: &'static str, hex: &str) -> Result<ContentDigest, RenderPlanError> {
    ContentDigest::from_hex(hex)
        .map_err(|source| RenderPlanError::MalformedDigest { field, source })
}

fn contribution_order(left: &RenderContribution, right: &RenderContribution) -> Ordering {
    kind_rank(left.kind())
        .cmp(&kind_rank(right.kind()))
        .then_with(|| {
            left.payload_contract_uri()
                .cmp(right.payload_contract_uri())
        })
        .then_with(|| left.claimant().cmp(right.claimant()))
}

fn kind_rank(kind: ContributionKind) -> u8 {
    match kind {
        ContributionKind::Component => 0,
        ContributionKind::Resource => 1,
    }
}
