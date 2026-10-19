//! Route declaration decode, getter, ordering, and strictness tests.

mod common;

use common::{contract_bytes, valid_contract};
use serde_json::json;
use worldrunner_render_plan::{
    BindingElementPolicy, CapabilityRouteDeclaration, ComponentRouteDeclaration, ContractDocument,
    ContractDocumentError, DocumentRouteDeclaration, RouteCardinality, RouteDeclarations,
    RouteIdentity,
};

fn decode(contract: &serde_json::Value) -> Result<ContractDocument, ContractDocumentError> {
    ContractDocument::from_json_bytes(&contract_bytes(contract))
}

fn reject(contract: &serde_json::Value) -> ContractDocumentError {
    decode(contract).expect_err("mutated fixture must fail")
}

// The route/field rejection matrix is deliberately branchy: each arm rejects one mutated
// route at a time so the specific error surface is pinned.
#[allow(clippy::cognitive_complexity)]
#[test]
fn valid_routes_decode_and_report_exact_authored_parts() {
    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("valid fixture must decode");
    let routes = document.routes();
    let document_route = routes.document();
    assert_eq!(
        document_route.schema_uri(),
        "synthetic:schema/document-alpha"
    );
    assert_eq!(document_route.schema_version(), 1);
    assert_eq!(
        document_route.identity().handler_factory_uri(),
        "synthetic:factory/document-handler"
    );
    assert_eq!(
        document_route.identity().handler_id(),
        "synthetic-document-handler"
    );
    assert_eq!(document_route.identity().handler_revision(), 3);
    assert_eq!(
        document_route.identity().payload_contract_uri(),
        "synthetic:payload/document-payload"
    );
    assert_eq!(
        document_route.identity().role_uri(),
        "synthetic:role/document"
    );

    let mesh = routes
        .components()
        .iter()
        .find(|route| route.schema_uri() == "synthetic:schema/component-mesh")
        .expect("mesh component route is authored");
    assert_eq!(mesh.identity().handler_id(), "synthetic-mesh-handler");
    assert_eq!(mesh.cardinality().minimum(), 0);
    assert_eq!(mesh.cardinality().maximum(), 16);

    let height = routes
        .capabilities()
        .iter()
        .find(|route| route.capability_uri() == "synthetic:capability/terrain-height")
        .expect("height capability route is authored");
    assert_eq!(height.identity().role_uri(), "synthetic:role/terrain");
    assert_eq!(height.cardinality().minimum(), 1);
    assert_eq!(height.cardinality().maximum(), 1);
    assert_eq!(
        height.element_policy(),
        &BindingElementPolicy::Exact {
            element_uri: "synthetic:element/terrain-height".to_owned()
        }
    );
    assert_eq!(
        routes.capabilities()[0].element_policy(),
        &BindingElementPolicy::Bare {}
    );
}

#[test]
fn zero_handler_revision_is_a_valid_authored_revision() {
    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("valid fixture must decode");
    let water = document
        .routes()
        .components()
        .iter()
        .find(|route| route.schema_uri() == "synthetic:schema/component-water")
        .expect("water component route is authored");
    assert_eq!(water.identity().handler_revision(), 0);

    let mut contract = valid_contract();
    contract["routes"]["document"]["handler_revision"] = json!(0);
    assert!(decode(&contract).is_ok());
}

#[test]
fn missing_mandatory_route_parts_are_rejected() {
    let mut contract = valid_contract();
    contract["routes"]
        .as_object_mut()
        .expect("routes")
        .remove("document");
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    for part in ["components", "capabilities"] {
        let mut contract = valid_contract();
        contract["routes"]
            .as_object_mut()
            .expect("routes")
            .remove(part);
        assert!(
            matches!(reject(&contract), ContractDocumentError::Json(_)),
            "removing routes {part} must fail as strict JSON"
        );
    }

    for field in [
        "schema_uri",
        "schema_version",
        "handler_factory_uri",
        "handler_id",
        "handler_revision",
        "payload_contract_uri",
        "role_uri",
    ] {
        let mut contract = valid_contract();
        contract["routes"]["document"]
            .as_object_mut()
            .expect("document route")
            .remove(field);
        assert!(
            matches!(reject(&contract), ContractDocumentError::Json(_)),
            "removing document route {field} must fail as strict JSON"
        );
    }

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][0]
        .as_object_mut()
        .expect("capability route")
        .remove("element_policy");
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));
}

#[test]
// The rejected-variant arms are deliberately branchy: each arm pins one unknown field or
// enum variant that must fail to decode.
#[allow(clippy::cognitive_complexity)]
fn unknown_route_fields_and_variants_are_rejected() {
    let mut contract = valid_contract();
    contract["routes"]["surprise"] = json!(1);
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["routes"]["document"]["surprise"] = json!(true);
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["routes"]["components"][0]["surprise"] = json!(1);
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][1]["surprise"] = json!(1);
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][0]["cardinality"]["surprise"] = json!(1);
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][0]["element_policy"]["kind"] = json!("element");
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][0]["element_policy"]["element_uri"] =
        json!("synthetic:element/stray");
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][1]["element_policy"]["extra"] = json!(1);
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][1]["element_policy"]
        .as_object_mut()
        .expect("element policy")
        .remove("element_uri");
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));
}

#[test]
fn malformed_route_uris_and_handler_ids_are_rejected() {
    let mut contract = valid_contract();
    contract["routes"]["document"]["schema_uri"] = json!("no-scheme-schema");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidUri {
            field: "document route schema_uri",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["routes"]["document"]["handler_factory_uri"] = json!("factory");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidUri {
            field: "route handler_factory_uri",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["routes"]["document"]["handler_id"] = json!("");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidIdentifier {
            field: "route handler_id",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["routes"]["document"]["payload_contract_uri"] = json!("payload");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidUri {
            field: "route payload_contract_uri",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["routes"]["components"][0]["role_uri"] = json!("role");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidUri {
            field: "route role_uri",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][0]["capability_uri"] = json!("capability");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidUri {
            field: "capability route capability_uri",
            ..
        }
    ));
}

#[test]
fn zero_document_route_schema_version_is_rejected() {
    let mut contract = valid_contract();
    contract["routes"]["document"]["schema_version"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "document route schema_version",
            ..
        }
    ));
}

#[test]
fn degenerate_cardinality_bounds_are_rejected() {
    let mut contract = valid_contract();
    contract["routes"]["components"][0]["cardinality"]["maximum"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "route cardinality maximum",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][0]["cardinality"]["minimum"] = json!(2);
    contract["routes"]["capabilities"][0]["cardinality"]["maximum"] = json!(1);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::ViolatedInvariant {
            field: "route cardinality",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"][0]["cardinality"]
        .as_object_mut()
        .expect("cardinality")
        .remove("minimum");
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    assert!(matches!(
        RouteCardinality::new(0, 0),
        Err(ContractDocumentError::OutOfRange { .. })
    ));
    assert!(matches!(
        RouteCardinality::new(3, 2),
        Err(ContractDocumentError::ViolatedInvariant { .. })
    ));
    assert!(RouteCardinality::new(0, 1).is_ok());
}

#[test]
fn exact_element_policy_uri_must_be_structural() {
    let mut contract = valid_contract();
    contract["routes"]["capabilities"][1]["element_policy"]["element_uri"] = json!("no-element");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidUri {
            field: "element policy element_uri",
            ..
        }
    ));

    let exact = BindingElementPolicy::Exact {
        element_uri: "synthetic:element/alpha".to_owned(),
    };
    assert!(exact.validate().is_ok());
    let malformed = BindingElementPolicy::Exact {
        element_uri: String::new(),
    };
    assert!(matches!(
        malformed.validate(),
        Err(ContractDocumentError::InvalidUri { .. })
    ));
    assert!(BindingElementPolicy::Bare {}.validate().is_ok());
}

#[test]
fn unsorted_route_keys_are_rejected() {
    let mut contract = valid_contract();
    contract["routes"]["components"] = json!([
        contract["routes"]["components"][1].clone(),
        contract["routes"]["components"][0].clone(),
    ]);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnsortedEntries {
            field: "route components"
        }
    ));

    let mut contract = valid_contract();
    contract["routes"]["capabilities"] = json!([
        contract["routes"]["capabilities"][1].clone(),
        contract["routes"]["capabilities"][0].clone(),
    ]);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnsortedEntries {
            field: "route capabilities"
        }
    ));
}

#[test]
fn duplicate_route_keys_are_rejected() {
    let mut contract = valid_contract();
    let duplicate = contract["routes"]["components"][1].clone();
    contract["routes"]["components"]
        .as_array_mut()
        .expect("component routes")
        .push(duplicate);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::DuplicateKey {
            field: "route components",
            ..
        }
    ));

    let mut contract = valid_contract();
    let duplicate = contract["routes"]["capabilities"][1].clone();
    contract["routes"]["capabilities"]
        .as_array_mut()
        .expect("capability routes")
        .push(duplicate);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::DuplicateKey {
            field: "route capabilities",
            ..
        }
    ));
}

#[test]
fn empty_component_routes_are_accepted() {
    let mut contract = valid_contract();
    contract["routes"]["components"] = json!([]);
    let bytes = contract_bytes(&contract);
    let document = ContractDocument::from_json_bytes(&bytes).expect("empty component list decodes");
    assert!(document.routes().components().is_empty());
    assert_eq!(document.routes().capabilities().len(), 2);
}

#[test]
fn empty_capability_routes_fail_only_through_asset_presence_closure() {
    let identity = RouteIdentity::new(
        "synthetic:factory/document-handler".to_owned(),
        "synthetic-document-handler".to_owned(),
        3,
        "synthetic:payload/document-payload".to_owned(),
        "synthetic:role/document".to_owned(),
    )
    .expect("valid authored identity constructs");
    let document_route =
        DocumentRouteDeclaration::new("synthetic:schema/document-alpha".to_owned(), 1, identity)
            .expect("valid document route constructs");
    let routes = RouteDeclarations::new(document_route, Vec::new(), Vec::new())
        .expect("pure route schema accepts empty capability routes");
    assert!(routes.validate().is_ok());

    let mut contract = valid_contract();
    contract["routes"]["capabilities"] = json!([]);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::ViolatedInvariant {
            field: "routes capabilities",
            ..
        }
    ));
}

#[test]
fn asset_binding_roles_are_structural_and_shareable() {
    let mut contract = valid_contract();
    contract["assets"]["bindings"][0]["role_uri"] = json!("role");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidUri {
            field: "asset binding role_uri",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["assets"]["bindings"][0]
        .as_object_mut()
        .expect("binding")
        .remove("role_uri");
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("shared roles decode");
    let terrain_roles = document
        .assets()
        .bindings()
        .iter()
        .map(worldrunner_render_plan::AssetBinding::role_uri)
        .filter(|role| *role == "synthetic:role/terrain")
        .count();
    assert_eq!(terrain_roles, 2);
}

#[test]
fn typed_route_construction_validates_as_strictly_as_decoding() {
    let identity = RouteIdentity::new(
        "synthetic:factory/mesh-handler".to_owned(),
        "synthetic-mesh-handler".to_owned(),
        0,
        "synthetic:payload/mesh-payload".to_owned(),
        "synthetic:role/mesh".to_owned(),
    )
    .expect("valid authored identity constructs");
    let document_route = DocumentRouteDeclaration::new(
        "synthetic:schema/document-alpha".to_owned(),
        1,
        identity.clone(),
    )
    .expect("valid document route constructs");
    let component = ComponentRouteDeclaration::new(
        "synthetic:schema/component-mesh".to_owned(),
        identity.clone(),
        RouteCardinality::new(0, 16).expect("valid cardinality"),
    )
    .expect("valid component route constructs");
    let capability = CapabilityRouteDeclaration::new(
        "synthetic:capability/mesh-artifact".to_owned(),
        identity,
        RouteCardinality::new(1, 16).expect("valid cardinality"),
        BindingElementPolicy::Bare {},
    )
    .expect("valid capability route constructs");
    assert!(RouteDeclarations::new(document_route, vec![component], vec![capability]).is_ok());

    let bad_factory = RouteIdentity::new(
        "not-a-uri".to_owned(),
        "synthetic-document-handler".to_owned(),
        0,
        "synthetic:payload/document-payload".to_owned(),
        "synthetic:role/document".to_owned(),
    );
    assert!(matches!(
        bad_factory,
        Err(ContractDocumentError::InvalidUri { .. })
    ));
}
