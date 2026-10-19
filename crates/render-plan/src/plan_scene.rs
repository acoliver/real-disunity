//! Scene composition phase of render world plan minting.
//!
//! The phase runs strictly after route, handler, payload, and whole-plan
//! closure: every adapted contribution is mapped back to its exact authored
//! declaration, its activation-bound route scene factory produces one closed
//! renderer-domain contribution with exact claimant provenance, and the
//! activation-bound document composer folds the exact retained contract
//! document, the root payload, and all contributions into one validated
//! [`RenderScene`].

use worldrunner_world_model::SemanticWorldDocument;
use worldrunner_world_runtime::ContributionKind;

use crate::adapter::RenderPayload;
use crate::document::{CapabilityRouteDeclaration, ComponentRouteDeclaration, ContractDocument};
use crate::error::RenderPlanError;
use crate::implementation_registry::{ActivatedRenderWorldContract, RenderRouteKind};
use crate::plan::{RenderContribution, claimant_route_key};
use crate::scene::{
    DocumentSceneInput, ProvenSceneContribution, RenderScene, RouteDeclarationRef,
    RouteSceneFactory, RouteSceneInput, SceneContribution,
};
use std::sync::Arc;

/// Composes the mandatory scene of one plan from its adapted contributions.
///
/// Contributions are processed in the plan's deterministic sorted order, so
/// the composer observes reproducible provenance regardless of publication
/// iteration order.
pub(crate) fn compose_scene(
    world: &SemanticWorldDocument,
    activated: &ActivatedRenderWorldContract,
    root_payload: &RenderPayload,
    contributions: &[RenderContribution],
) -> Result<RenderScene, RenderPlanError> {
    let authored = activated.authored_contract();
    let mut proven = Vec::with_capacity(contributions.len());
    for contribution in contributions {
        let scene = compose_contribution(world, activated, contribution)?;
        proven.push(ProvenSceneContribution::new(
            contribution.claimant().to_owned(),
            scene,
        ));
    }
    let composer = activated.document_scene_composer();
    let scene = composer
        .compose_scene(DocumentSceneInput::new(authored, root_payload, &proven))
        .map_err(|source| RenderPlanError::SceneCompositionRejected {
            handler_factory_uri: authored
                .routes()
                .document()
                .identity()
                .handler_factory_uri()
                .to_owned(),
            reason: source.reason().to_owned(),
        })?;
    scene
        .validate()
        .map_err(|source| RenderPlanError::SceneValidation { source })?;
    Ok(scene)
}

/// Produces one contribution through its exact activation-bound route factory.
fn compose_contribution(
    world: &SemanticWorldDocument,
    activated: &ActivatedRenderWorldContract,
    contribution: &RenderContribution,
) -> Result<SceneContribution, RenderPlanError> {
    let authored = activated.authored_contract();
    match contribution.kind() {
        ContributionKind::Component => {
            let route_key =
                claimant_route_key(contribution.kind(), contribution.claimant(), world)?;
            let route = authored_declaration_component(authored, &route_key)?;
            let factory = activated
                .component_scene_factory(&route_key)
                .ok_or_else(|| scene_route_mismatch(RenderRouteKind::Component, &route_key))?;
            invoke_factory(
                RenderRouteKind::Component,
                &route_key,
                route.identity().handler_factory_uri(),
                contribution,
                RouteDeclarationRef::Component(route),
                factory,
            )
        }
        ContributionKind::Resource => {
            let route_key =
                claimant_route_key(contribution.kind(), contribution.claimant(), world)?;
            let route = authored_declaration_capability(authored, &route_key)?;
            let factory = activated
                .capability_scene_factory(&route_key)
                .ok_or_else(|| scene_route_mismatch(RenderRouteKind::Capability, &route_key))?;
            invoke_factory(
                RenderRouteKind::Capability,
                &route_key,
                route.identity().handler_factory_uri(),
                contribution,
                RouteDeclarationRef::Capability(route),
                factory,
            )
        }
    }
}

/// Finds the exact authored component declaration of one route key.
fn authored_declaration_component<'a>(
    document: &'a ContractDocument,
    route_key: &str,
) -> Result<&'a ComponentRouteDeclaration, RenderPlanError> {
    document
        .routes()
        .components()
        .iter()
        .find(|route| route.schema_uri() == route_key)
        .ok_or_else(|| scene_route_mismatch(RenderRouteKind::Component, route_key))
}

/// Finds the exact authored capability declaration of one route key.
fn authored_declaration_capability<'a>(
    document: &'a ContractDocument,
    route_key: &str,
) -> Result<&'a CapabilityRouteDeclaration, RenderPlanError> {
    document
        .routes()
        .capabilities()
        .iter()
        .find(|route| route.capability_uri() == route_key)
        .ok_or_else(|| scene_route_mismatch(RenderRouteKind::Capability, route_key))
}

/// Invokes one activation-bound route scene factory with the exact context.
fn invoke_factory(
    route_kind: RenderRouteKind,
    route_key: &str,
    handler_factory_uri: &str,
    contribution: &RenderContribution,
    declaration: RouteDeclarationRef<'_>,
    factory: &Arc<dyn RouteSceneFactory>,
) -> Result<SceneContribution, RenderPlanError> {
    factory
        .compose_contribution(RouteSceneInput::new(declaration, contribution.payload()))
        .map_err(|source| RenderPlanError::SceneFactoryRejected {
            route_kind,
            route_key: route_key.to_owned(),
            handler_factory_uri: handler_factory_uri.to_owned(),
            claimant: contribution.claimant().to_owned(),
            reason: source.reason().to_owned(),
        })
}

/// The scene-phase route mismatch for one route key.
///
/// The whole-plan closure phase already proved every present route's
/// declaration and scene factory, so this failure is unreachable in practice
/// but keeps the scene phase independently fail-closed.
fn scene_route_mismatch(route_kind: RenderRouteKind, route_key: &str) -> RenderPlanError {
    RenderPlanError::AuthoredRouteMismatch {
        route_kind,
        route_key: route_key.to_owned(),
        field: "scene factory",
    }
}
