//! Shared route resolution, payload adaptation and preparation limits.

use super::{
    Arc, BTreeSet, ContentDigest, ContributionKind, Ordering, PreparedContribution,
    PreparedSemanticWorld, RenderContribution, RenderPayload, RenderPayloadAdapter,
    RenderPlanError, RenderWorldContract, RenderWorldLimits, SemanticWorldDocument,
};

/// Records which payload contracts each adapter kind actually consumed.
#[derive(Default)]
pub(super) struct AdapterUse {
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
    route_key: String,
    adapter: &'a Arc<dyn RenderPayloadAdapter>,
}

pub(super) fn enforce_contribution_limits(
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

pub(super) fn adapt_contributions(
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
            entry.route_key,
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
            route_key,
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

pub(super) fn verify_adapter_use(
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

pub(super) fn adapt_root(
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

pub(super) fn parse_digest(
    field: &'static str,
    hex: &str,
) -> Result<ContentDigest, RenderPlanError> {
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
