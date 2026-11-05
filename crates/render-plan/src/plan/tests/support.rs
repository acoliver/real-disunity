//! Shared synthetic route implementations and publication inputs.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use worldrunner_world_model::{
    DecodedSave, ResourceBinding, SchemaDescriptor, SemanticComponent, SemanticMap,
    SemanticWorldDocument, SourceIdentity, ValidationLimits,
};
use worldrunner_world_runtime::{
    BindingRequirement, ComponentHandler, ComponentPlan, ComponentRejection, ContributionKind,
    ContributionRequirement, DocumentHandler, DocumentPlan, DocumentRejection, HandlerDescriptor,
    PreparedPayload, RuntimePlanLimits, VerifiedNormalPublication,
};

use super::{common, publication};
use crate::*;

pub const LEFT: &str = "synthetic:capability/left";
pub const RIGHT: &str = "synthetic:capability/right";
const COMPONENT: &str = "synthetic:schema/join";
const PAYLOAD: &str = "synthetic:payload/bytes";
const DOCUMENT_FACTORY: &str = "synthetic:factory/document-handler";
const COMPONENT_FACTORY: &str = "synthetic:factory/join";
const BYTES_FACTORY: &str = "synthetic:factory/bytes";

pub fn document_only_registry() -> RenderImplementationRegistry {
    let factory = Arc::new(factories(false, false));
    let mut builder = RenderImplementationRegistryBuilder::new();
    builder.document_factory(DOCUMENT_FACTORY, factory).unwrap();
    builder.build()
}

pub struct Fixture {
    pub document: ContractDocument,
    pub publication: VerifiedNormalPublication,
    pub limits: RenderWorldLimits,
    _directory: publication::Directory,
}

impl Fixture {
    pub fn new(value: &Value) -> Self {
        let document = ContractDocument::from_json_bytes(&common::contract_bytes(value)).unwrap();
        let bytes = common::contract_bytes(value);
        let world = world(&bytes);
        let (publication, directory) = publication::verify(&world, &bytes);
        Self {
            document,
            publication,
            limits: limits(),
            _directory: directory,
        }
    }
}

pub fn validation_limits() -> ValidationLimits {
    ValidationLimits {
        max_depth: 8,
        max_nodes: 128,
        max_text_bytes: 4096,
    }
}

fn limits() -> RenderWorldLimits {
    RenderWorldLimits::new(
        RuntimePlanLimits::new(validation_limits(), 8, 8, 1_000_000, 2_000_000),
        8,
        8,
    )
}

pub fn configuration() -> Value {
    let mut value = common::valid_contract();
    value["scene"]["meshes"] = json!([]);
    value["terrain"]["height_asset_uri"] = json!("synthetic:asset/left");
    value["terrain"]["material_palette"][0]["material_uri"] = json!("synthetic:asset/right");
    value["environment"]["lod_policies"] = json!([]);
    value["effects"]["water"]["asset_uri"] = json!("synthetic:asset/left");
    value["effects"]["sky"]["asset_uri"] = json!("synthetic:asset/right");
    value["routes"]["components"] = json!([{
        "schema_uri": COMPONENT, "handler_factory_uri": COMPONENT_FACTORY,
        "handler_id": "join-handler", "handler_revision": 1,
        "payload_contract_uri": "synthetic:payload/join", "role_uri": "synthetic:role/join",
        "cardinality": { "minimum": 1, "maximum": 1 }
    }]);
    value["routes"]["capabilities"] = json!([
        capability(LEFT),
        capability(RIGHT),
        capability(RENDER_CONTRACT_CAPABILITY_URI)
    ]);
    value["assets"]["bindings"] = json!([
        common::asset_binding(
            "contract",
            "synthetic:asset/contract",
            "synthetic:role/bytes"
        ),
        common::asset_binding("left", "synthetic:asset/left", "synthetic:role/bytes"),
        common::asset_binding("right", "synthetic:asset/right", "synthetic:role/bytes")
    ]);
    value
}

fn capability(uri: &str) -> Value {
    json!({ "capability_uri": uri, "handler_factory_uri": BYTES_FACTORY,
        "handler_id": "bytes-handler", "handler_revision": 1,
        "payload_contract_uri": PAYLOAD, "role_uri": "synthetic:role/bytes",
        "cardinality": { "minimum": 1, "maximum": 2 }, "element_policy": { "kind": "bare" }
    })
}

fn world(contract_bytes: &[u8]) -> SemanticWorldDocument {
    let components = vec![SemanticComponent {
        component_id: "join".to_owned(),
        schema_uri: COMPONENT.to_owned(),
        payload: SemanticMap::new(std::collections::BTreeMap::new()),
    }];
    let bindings = vec![
        binding("contract", RENDER_CONTRACT_CAPABILITY_URI, contract_bytes),
        binding("left", LEFT, &[3, 5]),
        binding("right", RIGHT, &[7, 11]),
    ];
    let source =
        SourceIdentity::of(&DecodedSave::new(components.clone(), &validation_limits()).unwrap())
            .unwrap();
    SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: "synthetic:schema/document-alpha".to_owned(),
            version: 1,
        },
        source,
        components,
        bindings,
        &validation_limits(),
    )
    .unwrap()
}

fn binding(id: &str, capability: &str, bytes: &[u8]) -> ResourceBinding {
    ResourceBinding::new(
        id,
        capability,
        publication::digest(bytes),
        u64::try_from(bytes.len()).unwrap(),
        None,
    )
    .unwrap()
}

struct Root(HandlerDescriptor);
impl DocumentHandler for Root {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.0
    }
    fn plan(&self, _document: &SemanticWorldDocument) -> Result<DocumentPlan, DocumentRejection> {
        Ok(DocumentPlan::new(
            self.0.clone(),
            17_u32,
            vec![
                ContributionRequirement::new(
                    ContributionKind::Component,
                    "join",
                    "synthetic:payload/join",
                ),
                ContributionRequirement::new(ContributionKind::Resource, "contract", PAYLOAD),
                ContributionRequirement::new(ContributionKind::Resource, "left", PAYLOAD),
                ContributionRequirement::new(ContributionKind::Resource, "right", PAYLOAD),
            ],
        ))
    }
}

struct Join(HandlerDescriptor);
impl ComponentHandler for Join {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.0
    }
    fn plan(&self, _component: &SemanticComponent) -> Result<ComponentPlan, ComponentRejection> {
        Ok(ComponentPlan::new(
            self.0.clone(),
            "synthetic:payload/join",
            17_u32,
            vec![
                BindingRequirement::new("contract", RENDER_CONTRACT_CAPABILITY_URI, None),
                BindingRequirement::new("left", LEFT, None),
                BindingRequirement::new("right", RIGHT, None),
            ],
        ))
    }
}

struct Adapter {
    reject: bool,
}
impl RenderPayloadAdapter for Adapter {
    fn adapt(&self, payload: &PreparedPayload) -> Result<RenderPayload, RenderAdapterError> {
        if self.reject {
            return Err(RenderAdapterError::Rejected {
                reason: "synthetic adaptation refusal".to_owned(),
            });
        }
        if let Some(bytes) = payload.downcast_ref::<ExactBytes>() {
            return Ok(RenderPayload::new(bytes.bytes().to_vec()));
        }
        Ok(RenderPayload::new(*payload.downcast_ref::<u32>().unwrap()))
    }
}

struct Factories {
    scene: bool,
    omitted_scene: Option<RenderRouteKind>,
    scene_calls: Arc<AtomicUsize>,
    adapter: Arc<dyn RenderPayloadAdapter>,
}
impl Factories {
    fn has_scene(&self, kind: RenderRouteKind) -> bool {
        self.scene && self.omitted_scene != Some(kind)
    }
}
impl DocumentImplementationFactory for Factories {
    fn build_implementation(
        &self,
        route: &DocumentRouteDeclaration,
    ) -> Result<DocumentImplementation, RenderFactoryError> {
        let identity = route.identity();
        let implementation = DocumentImplementation::new(
            Arc::new(Root(HandlerDescriptor::new(
                identity.handler_id(),
                identity.handler_revision(),
            ))),
            Arc::clone(&self.adapter),
        );
        Ok(if self.has_scene(RenderRouteKind::Document) {
            implementation.with_scene_composer(Arc::new(Composer(Arc::clone(&self.scene_calls))))
        } else {
            implementation
        })
    }
}
impl ComponentImplementationFactory for Factories {
    fn build_implementation(
        &self,
        route: &ComponentRouteDeclaration,
    ) -> Result<ComponentImplementation, RenderFactoryError> {
        let identity = route.identity();
        let implementation = ComponentImplementation::new(
            Arc::new(Join(HandlerDescriptor::new(
                identity.handler_id(),
                identity.handler_revision(),
            ))),
            Arc::clone(&self.adapter),
        );
        Ok(if self.has_scene(RenderRouteKind::Component) {
            implementation.with_scene_factory(Arc::new(NonRendering(Arc::clone(&self.scene_calls))))
        } else {
            implementation
        })
    }
}
impl CapabilityImplementationFactory for Factories {
    fn build_implementation(
        &self,
        route: &CapabilityRouteDeclaration,
    ) -> Result<CapabilityImplementation, RenderFactoryError> {
        let identity = route.identity();
        let handler = exact_bytes_handler(
            ExactCapabilityRoute::new(
                route.capability_uri(),
                identity.payload_contract_uri(),
                identity.handler_id(),
                identity.handler_revision(),
                ElementPolicy::Bare,
            )
            .unwrap(),
            1_000_000,
        );
        let implementation = CapabilityImplementation::new(handler, Arc::clone(&self.adapter));
        Ok(if self.has_scene(RenderRouteKind::Capability) {
            implementation.with_scene_factory(Arc::new(NonRendering(Arc::clone(&self.scene_calls))))
        } else {
            implementation
        })
    }
}

pub fn registry(scene: bool, reject: bool) -> RenderImplementationRegistry {
    build_registry(factories(scene, reject))
}

pub fn registry_without_scene(kind: RenderRouteKind) -> RenderImplementationRegistry {
    build_registry(Factories {
        omitted_scene: Some(kind),
        ..factories(true, false)
    })
}

pub fn scene_probe_registry() -> (RenderImplementationRegistry, Arc<AtomicUsize>) {
    let factories = factories(true, false);
    let probe = Arc::clone(&factories.scene_calls);
    (build_registry(factories), probe)
}

fn factories(scene: bool, reject: bool) -> Factories {
    Factories {
        scene,
        omitted_scene: None,
        scene_calls: Arc::new(AtomicUsize::new(0)),
        adapter: Arc::new(Adapter { reject }),
    }
}

fn build_registry(factories: Factories) -> RenderImplementationRegistry {
    let factory = Arc::new(factories);
    let mut builder = RenderImplementationRegistryBuilder::new();
    builder
        .document_factory(DOCUMENT_FACTORY, factory.clone())
        .unwrap();
    builder
        .component_factory(COMPONENT_FACTORY, factory.clone())
        .unwrap();
    builder.capability_factory(BYTES_FACTORY, factory).unwrap();
    builder.build()
}

struct NonRendering(Arc<AtomicUsize>);
impl RouteSceneFactory for NonRendering {
    fn compose_contribution(
        &self,
        _input: RouteSceneInput<'_>,
    ) -> Result<SceneContribution, RenderFactoryError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(SceneContribution::NonRendering)
    }
}

struct Composer(Arc<AtomicUsize>);
impl SceneCompositionFactory for Composer {
    fn compose_scene(
        &self,
        input: DocumentSceneInput<'_>,
    ) -> Result<RenderScene, RenderFactoryError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        assert!(
            input
                .contributions()
                .iter()
                .all(|entry| matches!(entry.contribution(), SceneContribution::NonRendering))
        );
        let mut heights = Vec::new();
        for route in [LEFT, RIGHT] {
            for payload in input
                .payloads_for_route(ContributionKind::Resource, route)
                .map_err(|error| RenderFactoryError::new(error.to_string()))?
            {
                heights.extend(
                    payload
                        .payload()
                        .downcast_ref::<Vec<u8>>()
                        .unwrap()
                        .iter()
                        .map(|value| i32::from(*value)),
                );
            }
        }
        scene_with_heights(heights).map_err(|error| RenderFactoryError::new(error.to_string()))
    }
}

fn scene_with_heights(heights: Vec<i32>) -> Result<RenderScene, SceneValidationError> {
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let texture = SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255].into_boxed_slice(),
    };
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: heights,
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![texture], vec![material]).unwrap(),
        geometry: SceneGeometry::new(Vec::new(), Vec::new()).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
}
