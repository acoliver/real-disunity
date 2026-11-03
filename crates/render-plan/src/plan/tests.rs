//! Synthetic preparation and composition boundary regressions.

mod authority;
#[path = "../../tests/common/mod.rs"]
mod common;
mod publication;
mod support;

#[test]
fn full_plan_accessors_remain_available_as_associated_functions() {
    let fixture = Fixture::new(&configuration());
    let activated =
        activate_render_world_contract(&fixture.document, &registry(true, false)).unwrap();
    let plan = build_render_world_plan(&fixture.publication, &activated, &fixture.limits).unwrap();
    assert_eq!(RenderWorldPlan::authored_contract(&plan), &fixture.document);
    assert_eq!(
        ActivatedRenderWorldContract::authored_contract(&activated),
        &fixture.document
    );
    assert_eq!(
        RenderWorldPlan::root_payload(&plan).downcast_ref::<u32>(),
        Some(&17)
    );
    assert_eq!(RenderWorldPlan::contributions(&plan).len(), 4);
}

#[test]
fn governed_preparation_retains_discovery_and_composes_after_scene_admission() {
    let fixture = Fixture::new(&configuration());
    let discovered = crate::discover_render_contract(
        &fixture.publication,
        crate::RenderContractDiscoveryLimits {
            max_contract_bytes: 1_000_000,
        },
    )
    .unwrap();
    let payload_activation =
        activate_render_payload_contract(discovered.document(), &registry(true, false)).unwrap();
    let prepared = prepare_discovered_render_world_payloads(
        &fixture.publication,
        &discovered,
        &payload_activation,
        &fixture.limits,
    )
    .unwrap();
    assert_eq!(prepared.governed_contract_binding_id(), Some("contract"));
    assert_eq!(
        prepared.governed_contract_address(),
        Some(discovered.address())
    );
    assert_eq!(
        prepared.generated_state_digest(),
        fixture.publication.publication().generated_state_digest()
    );
    let activated = payload_activation.into_render_contract().unwrap();
    drop(discovered);
    drop(fixture);
    let plan = compose_render_world_plan(prepared, &activated).unwrap();
    assert_eq!(plan.governed_contract_binding_id(), Some("contract"));
    assert_eq!(
        plan.scene().terrain().unwrap().heights_millimeters(),
        &[3, 5, 7, 11]
    );
}

#[test]
fn governed_preparation_and_full_build_reject_the_same_document_substitution() {
    let fixture = Fixture::new(&configuration());
    let discovered = crate::discover_render_contract(
        &fixture.publication,
        crate::RenderContractDiscoveryLimits {
            max_contract_bytes: 1_000_000,
        },
    )
    .unwrap();
    let mut substituted = configuration();
    substituted["contract_id"] = serde_json::json!("synthetic-substitution");
    let document =
        crate::ContractDocument::from_json_bytes(&common::contract_bytes(&substituted)).unwrap();
    let activated = activate_render_world_contract(&document, &registry(true, false)).unwrap();
    let preparation = prepare_discovered_render_world_payloads(
        &fixture.publication,
        &discovered,
        &activated,
        &fixture.limits,
    )
    .unwrap_err();
    let build = build_discovered_render_world_plan(
        &fixture.publication,
        &discovered,
        &activated,
        &fixture.limits,
    )
    .unwrap_err();
    assert!(matches!(
        preparation,
        RenderPlanError::GovernedContractSubstitution
    ));
    assert_eq!(preparation.to_string(), build.to_string());
}

#[test]
fn governed_entry_points_reject_discovery_from_a_different_publication() {
    let original = Fixture::new(&configuration());
    let mut changed = configuration();
    changed["contract_id"] = serde_json::json!("synthetic-other-publication");
    let other = Fixture::new(&changed);
    let discovered = crate::discover_render_contract(
        &original.publication,
        crate::RenderContractDiscoveryLimits {
            max_contract_bytes: 1_000_000,
        },
    )
    .unwrap();
    let other_discovered = crate::discover_render_contract(
        &other.publication,
        crate::RenderContractDiscoveryLimits {
            max_contract_bytes: 1_000_000,
        },
    )
    .unwrap();
    assert_eq!(discovered.binding_id(), other_discovered.binding_id());
    assert_ne!(discovered.address(), other_discovered.address());
    let activated =
        activate_render_world_contract(discovered.document(), &registry(true, false)).unwrap();
    build_discovered_render_world_plan(
        &original.publication,
        &discovered,
        &activated,
        &original.limits,
    )
    .unwrap();
    build_render_world_plan(&other.publication, &activated, &other.limits).unwrap();
    let preparation = prepare_discovered_render_world_payloads(
        &other.publication,
        &discovered,
        &activated,
        &other.limits,
    );
    let build = build_discovered_render_world_plan(
        &other.publication,
        &discovered,
        &activated,
        &other.limits,
    );
    assert!(matches!(
        preparation,
        Err(RenderPlanError::GovernedContractSubstitution)
    ));
    assert!(matches!(
        build,
        Err(RenderPlanError::GovernedContractSubstitution)
    ));
}

#[test]
fn full_activation_keeps_its_early_missing_scene_rejection() {
    let fixture = Fixture::new(&configuration());
    let registry = support::document_only_registry();
    assert!(matches!(
        activate_render_world_contract(&fixture.document, &registry),
        Err(crate::RenderImplementationError::MissingSceneFactory {
            route_kind: RenderRouteKind::Document,
            ..
        })
    ));
}
use super::*;
use crate::{RenderRouteKind, activate_render_payload_contract, activate_render_world_contract};
use support::{Fixture, configuration, registry};

#[test]
fn preparation_owns_route_qualified_payloads_without_invoking_scene_factories() {
    let fixture = Fixture::new(&configuration());
    let activated = activate_render_payload_contract(&fixture.document, &registry(false, false))
        .expect("payload activation needs no scene factories");
    let prepared = prepare_render_world_payloads(&fixture.publication, &activated, &fixture.limits)
        .expect("governed closure and adaptation pass");
    drop(fixture);
    drop(activated);
    let payloads = prepared
        .payloads_for_route(ContributionKind::Resource, support::LEFT)
        .unwrap();
    let values = payloads
        .map(|entry| entry.payload().downcast_ref::<Vec<u8>>().unwrap().clone())
        .collect::<Vec<_>>();
    assert_eq!(values, vec![vec![3, 5]]);
    assert_eq!(prepared.root_payload().downcast_ref::<u32>(), Some(&17));
    assert!(
        prepared
            .contributions()
            .iter()
            .any(|entry| entry.route_key() == support::RIGHT)
    );
}

#[test]
fn document_composer_joins_payloads_from_separate_routes_before_building_a_scene() {
    let fixture = Fixture::new(&configuration());
    let activated =
        activate_render_world_contract(&fixture.document, &registry(true, false)).unwrap();
    let prepared =
        prepare_render_world_payloads(&fixture.publication, &activated, &fixture.limits).unwrap();
    let plan = compose_render_world_plan(prepared, &activated).unwrap();
    assert_eq!(
        plan.scene().terrain().unwrap().heights_millimeters(),
        &[3, 5, 7, 11]
    );
    assert_eq!(plan.contributions().len(), 4);
}

#[test]
fn missing_prepared_route_is_rejected_with_its_exact_name() {
    let fixture = Fixture::new(&configuration());
    let activated =
        activate_render_payload_contract(&fixture.document, &registry(false, false)).unwrap();
    let prepared =
        prepare_render_world_payloads(&fixture.publication, &activated, &fixture.limits).unwrap();
    assert!(matches!(
        prepared.payloads_for_route(ContributionKind::Resource, "synthetic:capability/missing"),
        Err(RenderPlanError::AuthoredRouteMismatch { route_kind: RenderRouteKind::Capability, route_key, field: "prepared payload route" })
            if route_key == "synthetic:capability/missing"
    ));
}

#[test]
fn closure_and_adaptation_errors_are_shared_by_preparation_and_full_build() {
    let mut invalid = configuration();
    invalid["routes"]["capabilities"][0]["cardinality"]["minimum"] = serde_json::json!(2);
    let fixture = Fixture::new(&invalid);
    let activated =
        activate_render_world_contract(&fixture.document, &registry(true, false)).unwrap();
    let prepare = prepare_render_world_payloads(&fixture.publication, &activated, &fixture.limits)
        .unwrap_err();
    let build =
        build_render_world_plan(&fixture.publication, &activated, &fixture.limits).unwrap_err();
    assert!(matches!(
        prepare,
        RenderPlanError::RouteCardinalityViolation { .. }
    ));
    assert_eq!(prepare.to_string(), build.to_string());
    let fixture = Fixture::new(&configuration());
    let activated =
        activate_render_world_contract(&fixture.document, &registry(true, true)).unwrap();
    let prepare = prepare_render_world_payloads(&fixture.publication, &activated, &fixture.limits)
        .unwrap_err();
    let build =
        build_render_world_plan(&fixture.publication, &activated, &fixture.limits).unwrap_err();
    assert!(matches!(prepare, RenderPlanError::AdapterRejected { .. }));
    assert_eq!(prepare.to_string(), build.to_string());
}

#[test]
fn explicit_nonrendering_contributions_do_not_waive_required_scene_factories() {
    let fixture = Fixture::new(&configuration());
    let payload =
        activate_render_payload_contract(&fixture.document, &registry(false, false)).unwrap();
    assert!(matches!(
        payload.into_render_contract(),
        Err(crate::RenderImplementationError::MissingSceneFactory {
            route_kind: RenderRouteKind::Document,
            ..
        })
    ));
    let activated =
        activate_render_world_contract(&fixture.document, &registry(true, false)).unwrap();
    let plan = build_render_world_plan(&fixture.publication, &activated, &fixture.limits).unwrap();
    assert!(plan.scene().terrain().is_some());
}

#[test]
fn prepared_payloads_cannot_be_composed_through_a_different_activation() {
    let fixture = Fixture::new(&configuration());
    let activated =
        activate_render_world_contract(&fixture.document, &registry(true, false)).unwrap();
    let other = activate_render_world_contract(&fixture.document, &registry(true, false)).unwrap();
    let prepared =
        prepare_render_world_payloads(&fixture.publication, &activated, &fixture.limits).unwrap();
    assert!(matches!(
        compose_render_world_plan(prepared, &other),
        Err(RenderPlanError::AuthoredRouteMismatch {
            field: "prepared executable contract",
            ..
        })
    ));
}

#[test]
fn preparation_does_not_invoke_bound_scene_factories_or_the_composer() {
    let fixture = Fixture::new(&configuration());
    let (registry, probe) = support::scene_probe_registry();
    let activated = activate_render_world_contract(&fixture.document, &registry).unwrap();
    let prepared =
        prepare_render_world_payloads(&fixture.publication, &activated, &fixture.limits).unwrap();
    assert_eq!(probe.load(std::sync::atomic::Ordering::Relaxed), 0);
    compose_render_world_plan(prepared, &activated).unwrap();
    assert_eq!(probe.load(std::sync::atomic::Ordering::Relaxed), 5);
}

#[test]
fn deferred_render_admission_still_requires_every_declared_scene_factory() {
    let fixture = Fixture::new(&configuration());
    for kind in [
        RenderRouteKind::Document,
        RenderRouteKind::Component,
        RenderRouteKind::Capability,
    ] {
        let registry = support::registry_without_scene(kind);
        let activated = activate_render_payload_contract(&fixture.document, &registry).unwrap();
        assert!(matches!(activated.into_render_contract(),
            Err(crate::RenderImplementationError::MissingSceneFactory { route_kind, .. }) if route_kind == kind));
    }
}

#[test]
fn declared_idle_routes_are_empty_and_route_kind_selection_remains_exact() {
    let mut value = configuration();
    let mut idle = value["routes"]["capabilities"][0].clone();
    let key = "synthetic:capability/unused";
    idle["capability_uri"] = serde_json::json!(key);
    idle["cardinality"]["minimum"] = serde_json::json!(0);
    value["routes"]["capabilities"]
        .as_array_mut()
        .unwrap()
        .insert(2, idle);
    let fixture = Fixture::new(&value);
    let activated =
        activate_render_payload_contract(&fixture.document, &registry(false, false)).unwrap();
    let prepared =
        prepare_render_world_payloads(&fixture.publication, &activated, &fixture.limits).unwrap();
    assert_eq!(
        prepared
            .payloads_for_route(ContributionKind::Resource, key)
            .unwrap()
            .count(),
        0
    );
    assert!(
        matches!(prepared.payloads_for_route(ContributionKind::Component, key),
        Err(RenderPlanError::AuthoredRouteMismatch { route_kind: RenderRouteKind::Component, route_key, .. }) if route_key == key)
    );
}
