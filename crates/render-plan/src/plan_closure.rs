//! Exact authored-contract closure over one verified semantic world.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_world_model::{ResourceBinding, SemanticWorldDocument};

use crate::contract::RenderWorldContract;
use crate::document::{
    BindingElementPolicy, CapabilityRouteDeclaration, ComponentRouteDeclaration, ContractDocument,
    RouteCardinality, RouteIdentity,
};
use crate::error::RenderPlanError;
use crate::implementation_registry::RenderRouteKind;

pub(crate) fn validate_authored_plan_closure(
    world: &SemanticWorldDocument,
    authored: &ContractDocument,
    contract: &RenderWorldContract,
) -> Result<(), RenderPlanError> {
    authored
        .validate()
        .map_err(|source| RenderPlanError::AuthoredDocument { source })?;
    validate_document_route(world, authored, contract)?;
    validate_component_routes(world, authored, contract)?;
    validate_capability_routes(world, authored, contract)?;
    validate_asset_binding_closure(world, authored)
}

fn validate_document_route(
    world: &SemanticWorldDocument,
    authored: &ContractDocument,
    contract: &RenderWorldContract,
) -> Result<(), RenderPlanError> {
    let route = authored.routes().document();
    let key = route.schema_uri();
    require_route_field(
        route.schema_uri() == world.schema().tag
            && route.schema_version() == world.schema().version,
        RenderRouteKind::Document,
        key,
        "publication schema",
    )?;
    let registered = contract.runtime_registry().document_schema();
    require_route_field(
        registered.tag == route.schema_uri() && registered.version == route.schema_version(),
        RenderRouteKind::Document,
        key,
        "registered schema",
    )?;
    require_identity(
        RenderRouteKind::Document,
        key,
        route.identity(),
        contract.document_handler_descriptor(),
        contract.root_payload_contract_uri(),
    )
}

fn validate_component_routes(
    world: &SemanticWorldDocument,
    authored: &ContractDocument,
    contract: &RenderWorldContract,
) -> Result<(), RenderPlanError> {
    let authored_keys = authored
        .routes()
        .components()
        .iter()
        .map(ComponentRouteDeclaration::schema_uri)
        .collect::<BTreeSet<_>>();
    let registered_keys = contract
        .component_route_keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    require_exact_route_keys(
        RenderRouteKind::Component,
        "component-routes",
        &authored_keys,
        &registered_keys,
    )?;
    let counts = component_counts(world);
    for route in authored.routes().components() {
        let registered = contract
            .component_route(route.schema_uri())
            .ok_or_else(|| {
                route_mismatch(
                    RenderRouteKind::Component,
                    route.schema_uri(),
                    "registration",
                )
            })?;
        require_identity(
            RenderRouteKind::Component,
            route.schema_uri(),
            route.identity(),
            registered.handler().descriptor(),
            registered.payload_contract_uri(),
        )?;
        validate_cardinality(
            RenderRouteKind::Component,
            route.schema_uri(),
            route.cardinality(),
            counts.get(route.schema_uri()).copied().unwrap_or(0),
        )?;
    }
    Ok(())
}

fn validate_capability_routes(
    world: &SemanticWorldDocument,
    authored: &ContractDocument,
    contract: &RenderWorldContract,
) -> Result<(), RenderPlanError> {
    let authored_keys = authored
        .routes()
        .capabilities()
        .iter()
        .map(CapabilityRouteDeclaration::capability_uri)
        .collect::<BTreeSet<_>>();
    let registered_keys = contract
        .capability_route_keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    require_exact_route_keys(
        RenderRouteKind::Capability,
        "capability-routes",
        &authored_keys,
        &registered_keys,
    )?;
    let counts = capability_counts(world);
    for route in authored.routes().capabilities() {
        let registered = contract
            .capability_route(route.capability_uri())
            .ok_or_else(|| {
                route_mismatch(
                    RenderRouteKind::Capability,
                    route.capability_uri(),
                    "registration",
                )
            })?;
        require_identity(
            RenderRouteKind::Capability,
            route.capability_uri(),
            route.identity(),
            registered.handler().descriptor(),
            registered.payload_contract_uri(),
        )?;
        validate_cardinality(
            RenderRouteKind::Capability,
            route.capability_uri(),
            route.cardinality(),
            counts.get(route.capability_uri()).copied().unwrap_or(0),
        )?;
    }
    Ok(())
}

fn validate_asset_binding_closure(
    world: &SemanticWorldDocument,
    authored: &ContractDocument,
) -> Result<(), RenderPlanError> {
    let resources = world
        .bindings()
        .iter()
        .map(|binding| (binding.binding_id.as_str(), binding))
        .collect::<BTreeMap<_, _>>();
    let assets = authored
        .assets()
        .bindings()
        .iter()
        .map(|binding| (binding.binding_id(), binding))
        .collect::<BTreeMap<_, _>>();
    let resource_ids = resources.keys().copied().collect::<BTreeSet<_>>();
    let asset_ids = assets.keys().copied().collect::<BTreeSet<_>>();
    require_exact_asset_ids(&resource_ids, &asset_ids)?;
    for (binding_id, binding) in resources {
        let asset = assets.get(binding_id).ok_or_else(|| {
            asset_closure_error(binding_id, "is absent from the authored asset policy")
        })?;
        validate_binding_route(binding, asset.role_uri(), authored)?;
    }
    Ok(())
}

fn require_exact_asset_ids(
    resources: &BTreeSet<&str>,
    assets: &BTreeSet<&str>,
) -> Result<(), RenderPlanError> {
    if let Some(binding_id) = assets.difference(resources).next() {
        return Err(asset_closure_error(
            binding_id,
            "is absent from the publication",
        ));
    }
    if let Some(binding_id) = resources.difference(assets).next() {
        return Err(asset_closure_error(
            binding_id,
            "is absent from the authored asset policy",
        ));
    }
    Ok(())
}
fn validate_binding_route(
    binding: &ResourceBinding,
    asset_role_uri: &str,
    authored: &ContractDocument,
) -> Result<(), RenderPlanError> {
    let route = authored
        .routes()
        .capabilities()
        .iter()
        .find(|route| route.capability_uri() == binding.capability_uri)
        .ok_or_else(|| {
            route_mismatch(
                RenderRouteKind::Capability,
                &binding.capability_uri,
                "asset binding route",
            )
        })?;
    if !element_matches(route.element_policy(), binding.element_uri.as_deref()) {
        return Err(RenderPlanError::BindingElementPolicyMismatch {
            binding_id: binding.binding_id.clone(),
            capability_uri: binding.capability_uri.clone(),
        });
    }
    require_exact_role(
        &binding.binding_id,
        &binding.capability_uri,
        asset_role_uri,
        route.identity().role_uri(),
    )
}

fn require_exact_role(
    binding_id: &str,
    capability_uri: &str,
    asset_role_uri: &str,
    route_role_uri: &str,
) -> Result<(), RenderPlanError> {
    if asset_role_uri == route_role_uri {
        return Ok(());
    }
    Err(RenderPlanError::AssetRoleMismatch {
        binding_id: binding_id.to_owned(),
        asset_role_uri: asset_role_uri.to_owned(),
        capability_uri: capability_uri.to_owned(),
        route_role_uri: route_role_uri.to_owned(),
    })
}

fn component_counts(world: &SemanticWorldDocument) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for component in world.components() {
        *counts.entry(component.schema_uri.as_str()).or_insert(0) += 1;
    }
    counts
}

fn capability_counts(world: &SemanticWorldDocument) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for binding in world.bindings() {
        *counts.entry(binding.capability_uri.as_str()).or_insert(0) += 1;
    }
    counts
}

fn require_exact_route_keys(
    route_kind: RenderRouteKind,
    route_key: &str,
    authored: &BTreeSet<&str>,
    registered: &BTreeSet<&str>,
) -> Result<(), RenderPlanError> {
    require_route_field(
        authored == registered,
        route_kind,
        route_key,
        "exact route set",
    )
}
fn validate_cardinality(
    route_kind: RenderRouteKind,
    route_key: &str,
    cardinality: RouteCardinality,
    observed: usize,
) -> Result<(), RenderPlanError> {
    let observed =
        u64::try_from(observed).map_err(|_| RenderPlanError::RoutePopulationOverflow {
            route_kind,
            route_key: route_key.to_owned(),
        })?;
    if observed < cardinality.minimum() || observed > cardinality.maximum() {
        return Err(RenderPlanError::RouteCardinalityViolation {
            route_kind,
            route_key: route_key.to_owned(),
            minimum: cardinality.minimum(),
            maximum: cardinality.maximum(),
            observed,
        });
    }
    Ok(())
}

fn require_identity(
    route_kind: RenderRouteKind,
    route_key: &str,
    identity: &RouteIdentity,
    handler: &worldrunner_world_runtime::HandlerDescriptor,
    payload_contract_uri: &str,
) -> Result<(), RenderPlanError> {
    require_route_field(
        handler.handler_id == identity.handler_id()
            && handler.revision == identity.handler_revision(),
        route_kind,
        route_key,
        "handler identity",
    )?;
    require_route_field(
        payload_contract_uri == identity.payload_contract_uri(),
        route_kind,
        route_key,
        "payload contract URI",
    )
}

fn require_route_field(
    condition: bool,
    route_kind: RenderRouteKind,
    route_key: &str,
    field: &'static str,
) -> Result<(), RenderPlanError> {
    if condition {
        return Ok(());
    }
    Err(route_mismatch(route_kind, route_key, field))
}

fn route_mismatch(
    route_kind: RenderRouteKind,
    route_key: &str,
    field: &'static str,
) -> RenderPlanError {
    RenderPlanError::AuthoredRouteMismatch {
        route_kind,
        route_key: route_key.to_owned(),
        field,
    }
}

fn asset_closure_error(binding_id: &str, reason: &'static str) -> RenderPlanError {
    RenderPlanError::AssetBindingClosureMismatch {
        binding_id: binding_id.to_owned(),
        reason,
    }
}

fn element_matches(policy: &BindingElementPolicy, observed: Option<&str>) -> bool {
    match policy {
        BindingElementPolicy::Any {} => true,
        BindingElementPolicy::Bare {} => observed.is_none(),
        BindingElementPolicy::Exact { element_uri } => observed == Some(element_uri),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_world_runtime::HandlerDescriptor;

    fn exact_policy(uri: &str) -> BindingElementPolicy {
        serde_json::from_value(serde_json::json!({
            "kind": "exact",
            "element_uri": uri,
        }))
        .expect("exact test policy decodes")
    }

    fn identity() -> RouteIdentity {
        RouteIdentity::new(
            "synthetic:factory/alpha".to_owned(),
            "synthetic-handler-alpha".to_owned(),
            7,
            "synthetic:payload/alpha".to_owned(),
            "synthetic:role/alpha".to_owned(),
        )
        .expect("test route identity validates")
    }

    #[test]
    fn component_and_capability_cardinality_fail_closed() {
        let cardinality = RouteCardinality::new(1, 2).expect("valid bounds");
        assert!(
            validate_cardinality(
                RenderRouteKind::Component,
                "synthetic:schema/a",
                cardinality,
                1
            )
            .is_ok()
        );
        assert!(matches!(
            validate_cardinality(
                RenderRouteKind::Component,
                "synthetic:schema/a",
                cardinality,
                0
            ),
            Err(RenderPlanError::RouteCardinalityViolation {
                route_kind: RenderRouteKind::Component,
                observed: 0,
                ..
            })
        ));
        assert!(matches!(
            validate_cardinality(
                RenderRouteKind::Capability,
                "synthetic:capability/a",
                cardinality,
                3
            ),
            Err(RenderPlanError::RouteCardinalityViolation {
                route_kind: RenderRouteKind::Capability,
                observed: 3,
                ..
            })
        ));
    }

    #[test]
    fn element_policies_are_explicit_and_exact() {
        let any = BindingElementPolicy::Any {};
        let bare = BindingElementPolicy::Bare {};
        let exact = exact_policy("synthetic:element/a");
        assert!(element_matches(&any, None));
        assert!(element_matches(&any, Some("synthetic:element/other")));
        assert!(element_matches(&bare, None));
        assert!(!element_matches(&bare, Some("synthetic:element/a")));
        assert!(element_matches(&exact, Some("synthetic:element/a")));
        assert!(!element_matches(&exact, None));
        assert!(!element_matches(
            &exact,
            Some("synthetic:element/a-similar")
        ));
    }

    #[test]
    fn handler_and_payload_identity_drift_fail_closed() {
        let identity = identity();
        let faithful = HandlerDescriptor::new("synthetic-handler-alpha", 7);
        assert!(
            require_identity(
                RenderRouteKind::Document,
                "synthetic:schema/document",
                &identity,
                &faithful,
                "synthetic:payload/alpha",
            )
            .is_ok()
        );
        let drifted = HandlerDescriptor::new("synthetic-handler-alpha", 8);
        assert!(matches!(
            require_identity(
                RenderRouteKind::Document,
                "synthetic:schema/document",
                &identity,
                &drifted,
                "synthetic:payload/alpha",
            ),
            Err(RenderPlanError::AuthoredRouteMismatch {
                field: "handler identity",
                ..
            })
        ));
        assert!(matches!(
            require_identity(
                RenderRouteKind::Document,
                "synthetic:schema/document",
                &identity,
                &faithful,
                "synthetic:payload/other",
            ),
            Err(RenderPlanError::AuthoredRouteMismatch {
                field: "payload contract URI",
                ..
            })
        ));
    }

    #[test]
    fn route_sets_reject_similarly_spelled_uris() {
        let authored = BTreeSet::from(["synthetic:capability/alpha"]);
        let exact = BTreeSet::from(["synthetic:capability/alpha"]);
        assert!(
            require_exact_route_keys(
                RenderRouteKind::Capability,
                "capability-routes",
                &authored,
                &exact,
            )
            .is_ok()
        );
        let drifted = BTreeSet::from(["synthetic:capability/alpha-similar"]);
        assert!(matches!(
            require_exact_route_keys(
                RenderRouteKind::Capability,
                "capability-routes",
                &authored,
                &drifted,
            ),
            Err(RenderPlanError::AuthoredRouteMismatch {
                field: "exact route set",
                ..
            })
        ));
    }

    #[test]
    fn missing_and_extra_assets_have_distinct_closure_errors() {
        let resources = BTreeSet::from(["binding-a"]);
        let assets = BTreeSet::from(["binding-a", "binding-b"]);
        assert!(matches!(
            require_exact_asset_ids(&resources, &assets),
            Err(RenderPlanError::AssetBindingClosureMismatch {
                binding_id,
                reason: "is absent from the publication",
            }) if binding_id == "binding-b"
        ));
        let resources = BTreeSet::from(["binding-a", "binding-c"]);
        let assets = BTreeSet::from(["binding-a"]);
        assert!(matches!(
            require_exact_asset_ids(&resources, &assets),
            Err(RenderPlanError::AssetBindingClosureMismatch {
                binding_id,
                reason: "is absent from the authored asset policy",
            }) if binding_id == "binding-c"
        ));
    }

    #[test]
    fn opaque_role_mismatch_is_never_approximated() {
        assert!(
            require_exact_role(
                "binding-a",
                "synthetic:capability/a",
                "synthetic:role/alpha",
                "synthetic:role/alpha",
            )
            .is_ok()
        );
        assert!(matches!(
            require_exact_role(
                "binding-a",
                "synthetic:capability/a",
                "synthetic:role/alpha",
                "synthetic:role/alpha-similar",
            ),
            Err(RenderPlanError::AssetRoleMismatch {
                asset_role_uri,
                route_role_uri,
                ..
            }) if asset_role_uri != route_role_uri
        ));
    }
}
