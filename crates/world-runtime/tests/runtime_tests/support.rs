//! Shared arbitrary synthetic fixtures for world-runtime integration tests.
//!
//! Every schema, capability, handler, and identifier is invented for these
//! tests alone; nothing references datasets, corpora, or production URIs.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{self, Cursor, Read};
use std::sync::Arc;

use sha2::{Digest as Sha256Digest, Sha256};
use worldrunner_world_model::{
    ContentDigest, DecodedSave, ResourceBinding, SchemaDescriptor, SemanticComponent, SemanticMap,
    SemanticValue, SemanticWorldDocument, SourceIdentity, ValidationLimits,
};
use worldrunner_world_runtime::{
    BindingRequirement, CapabilityHandler, CapabilitySession, CapabilitySessionError,
    ComponentHandler, ComponentPlan, ComponentRejection, ContentAddress, ContributionKind,
    ContributionRequirement, DocumentHandler, DocumentPlan, DocumentRejection, HandlerDescriptor,
    PreparedSemanticWorld, ProviderError, ResourceContribution, ResourceProvider, RuntimePlanError,
    RuntimePlanLimits, RuntimePlanner, RuntimeRegistry, RuntimeRegistryBuilder,
};

pub const DOC_TAG: &str = "synthetic:world/negotiation";
pub const DOC_VERSION: u32 = 1;
pub const ALPHA_SCHEMA: &str = "synthetic:component/alpha";
pub const BETA_SCHEMA: &str = "synthetic:component/beta";
pub const BYTES_CAPABILITY: &str = "synthetic:capability/bytes";
pub const ELEMENT_CAPABILITY: &str = "synthetic:capability/element";
pub const ELEMENT_LEAF: &str = "synthetic:element/leaf";

pub fn limits() -> ValidationLimits {
    ValidationLimits {
        max_depth: 8,
        max_nodes: 64,
        max_text_bytes: 512,
    }
}

pub fn runtime_limits() -> RuntimePlanLimits {
    RuntimePlanLimits::new(limits(), 8, 8, 1_000_000, 2_000_000)
}

pub fn text(value: &str) -> SemanticValue {
    SemanticValue::Text(value.to_owned())
}

pub fn unsigned(value: u64) -> SemanticValue {
    SemanticValue::Unsigned(value)
}

pub fn binding_ref(binding_id: &str) -> SemanticValue {
    SemanticValue::BindingRef(binding_id.to_owned())
}

pub fn sequence(values: Vec<SemanticValue>) -> SemanticValue {
    SemanticValue::Sequence(values)
}

pub fn map(entries: &[(&str, SemanticValue)]) -> SemanticMap {
    SemanticMap::new(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

pub fn component(component_id: &str, schema_uri: &str, payload: SemanticMap) -> SemanticComponent {
    SemanticComponent {
        component_id: component_id.to_owned(),
        schema_uri: schema_uri.to_owned(),
        payload,
    }
}

pub fn alpha_payload(binding_id: &str) -> SemanticMap {
    map(&[
        ("mode", text("strict-alpha")),
        ("binding", binding_ref(binding_id)),
    ])
}

pub fn beta_payload(binding_id: &str) -> SemanticMap {
    map(&[
        ("mode", text("strict-beta")),
        ("binding", binding_ref(binding_id)),
    ])
}

pub fn alpha_multi_payload(binding_ids: &[&str]) -> SemanticMap {
    map(&[
        ("mode", text("strict-alpha")),
        (
            "bindings",
            sequence(binding_ids.iter().map(|id| binding_ref(id)).collect()),
        ),
    ])
}

pub fn digest_of(bytes: &[u8]) -> ContentDigest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    ContentDigest::from_bytes(hasher.finalize().into()).expect("sha-256 output is nonzero")
}

pub fn content_address(bytes: &[u8]) -> ContentAddress {
    ContentAddress::new(
        digest_of(bytes),
        u64::try_from(bytes.len()).expect("fixture length fits u64"),
    )
}

pub fn binding(
    binding_id: &str,
    capability_uri: &str,
    bytes: &[u8],
    element_uri: Option<&str>,
) -> ResourceBinding {
    ResourceBinding::new(
        binding_id,
        capability_uri,
        digest_of(bytes),
        u64::try_from(bytes.len()).expect("length fits u64"),
        element_uri,
    )
    .expect("valid binding")
}

pub fn document(
    components: Vec<SemanticComponent>,
    bindings: Vec<ResourceBinding>,
) -> SemanticWorldDocument {
    document_with_tag(DOC_TAG, DOC_VERSION, components, bindings)
}

pub fn document_with_tag(
    tag: &str,
    version: u32,
    components: Vec<SemanticComponent>,
    bindings: Vec<ResourceBinding>,
) -> SemanticWorldDocument {
    let save = DecodedSave::new(components.clone(), &limits()).expect("valid components");
    SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: tag.to_owned(),
            version,
        },
        SourceIdentity::of(&save).expect("identity derivation"),
        components,
        bindings,
        &limits(),
    )
    .expect("valid document")
}

#[derive(Clone, Copy)]
struct SyntheticContributionRoute {
    prefix: &'static str,
    override_uri: Option<&'static str>,
}

impl SyntheticContributionRoute {
    fn payload_contract_uri(self, claimant: &str) -> String {
        self.override_uri
            .map_or_else(|| format!("{}/{}", self.prefix, claimant), str::to_owned)
    }
}

struct SyntheticDocumentHandler {
    descriptor: HandlerDescriptor,
    components: HashMap<&'static str, SyntheticContributionRoute>,
    capabilities: HashMap<&'static str, SyntheticContributionRoute>,
}

impl SyntheticDocumentHandler {
    fn new(
        components: &[(&'static str, SyntheticComponentHandler)],
        capabilities: &[(&'static str, SyntheticCapabilityHandler)],
    ) -> Self {
        Self {
            descriptor: HandlerDescriptor::new("synthetic-handler/document", 13),
            components: components
                .iter()
                .map(|(schema, handler)| {
                    (
                        *schema,
                        SyntheticContributionRoute {
                            prefix: handler.contribution_prefix,
                            override_uri: handler.contribution_override,
                        },
                    )
                })
                .collect(),
            capabilities: capabilities
                .iter()
                .map(|(capability, handler)| {
                    (
                        *capability,
                        SyntheticContributionRoute {
                            prefix: handler.contribution_prefix,
                            override_uri: handler.contribution_override,
                        },
                    )
                })
                .collect(),
        }
    }
}

impl DocumentHandler for SyntheticDocumentHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn plan(&self, document: &SemanticWorldDocument) -> Result<DocumentPlan, DocumentRejection> {
        let mut requirements = Vec::new();
        for component in document.components() {
            if let Some(route) = self.components.get(component.schema_uri.as_str()) {
                requirements.push(ContributionRequirement::new(
                    ContributionKind::Component,
                    &component.component_id,
                    &route.payload_contract_uri(&component.component_id),
                ));
            }
        }
        for binding in document.bindings() {
            if let Some(route) = self.capabilities.get(binding.capability_uri.as_str()) {
                requirements.push(ContributionRequirement::new(
                    ContributionKind::Resource,
                    &binding.binding_id,
                    &route.payload_contract_uri(&binding.binding_id),
                ));
            }
        }
        requirements.sort();
        Ok(DocumentPlan::new(
            self.descriptor.clone(),
            map(&[("contract", text("strict-root"))]),
            requirements,
        ))
    }
}

pub struct SyntheticComponentHandler {
    pub descriptor: HandlerDescriptor,
    schema_uri: &'static str,
    required_capability: &'static str,
    required_element: Option<&'static str>,
    required_mode: &'static str,
    contribution_prefix: &'static str,
    contribution_override: Option<&'static str>,
    binding_override: Option<&'static str>,
    reported_identity: Option<HandlerDescriptor>,
}

impl SyntheticComponentHandler {
    pub fn new(
        handler_id: &'static str,
        revision: u64,
        schema_uri: &'static str,
        required_mode: &'static str,
    ) -> Self {
        Self {
            descriptor: HandlerDescriptor::new(handler_id, revision),
            schema_uri,
            required_capability: "",
            required_element: None,
            required_mode,
            contribution_prefix: "",
            contribution_override: None,
            binding_override: None,
            reported_identity: None,
        }
    }

    pub fn with_resource_contract(
        mut self,
        capability_uri: &'static str,
        element_uri: Option<&'static str>,
        contribution_prefix: &'static str,
    ) -> Self {
        self.required_capability = capability_uri;
        self.required_element = element_uri;
        self.contribution_prefix = contribution_prefix;
        self
    }

    pub fn with_contribution_override(mut self, uri: &'static str) -> Self {
        self.contribution_override = Some(uri);
        self
    }

    pub fn with_binding_override(mut self, binding_id: &'static str) -> Self {
        self.binding_override = Some(binding_id);
        self
    }

    pub fn with_reported_identity(mut self, handler_id: &'static str, revision: u64) -> Self {
        self.reported_identity = Some(HandlerDescriptor::new(handler_id, revision));
        self
    }
}

impl ComponentHandler for SyntheticComponentHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn plan(&self, component: &SemanticComponent) -> Result<ComponentPlan, ComponentRejection> {
        if component.schema_uri != self.schema_uri {
            return Err(ComponentRejection::new(
                "schema_uri",
                self.schema_uri,
                &component.schema_uri,
            ));
        }
        let observed = match component.payload.get("mode") {
            Some(value) => format!("{value:?}"),
            None => "absent".to_owned(),
        };
        let expected = format!("Text({:?})", self.required_mode);
        if observed != expected {
            return Err(ComponentRejection::new(
                "mode",
                self.required_mode,
                &observed,
            ));
        }
        let requirements = self.collect_requirements(component)?;
        let contribution = map(&[
            ("origin", text(&self.descriptor.handler_id)),
            ("claim", text(&component.component_id)),
        ]);
        let payload_contract_uri = match self.contribution_override {
            Some(uri) => uri.to_owned(),
            None => format!("{}/{}", self.contribution_prefix, component.component_id),
        };
        let handler = self
            .reported_identity
            .clone()
            .unwrap_or_else(|| self.descriptor.clone());
        Ok(ComponentPlan::new(
            handler,
            &payload_contract_uri,
            contribution,
            requirements,
        ))
    }
}

impl SyntheticComponentHandler {
    fn collect_requirements(
        &self,
        component: &SemanticComponent,
    ) -> Result<Vec<BindingRequirement>, ComponentRejection> {
        if let Some(value) = component.payload.get("bindings") {
            let items = match value {
                SemanticValue::Sequence(items) => items,
                other => {
                    return Err(ComponentRejection::new(
                        "bindings",
                        "sequence of binding references",
                        &format!("{other:?}"),
                    ));
                }
            };
            return items
                .iter()
                .map(|item| self.one_requirement(item))
                .collect();
        }
        match component.payload.get("binding") {
            Some(value) => Ok(vec![self.one_requirement(value)?]),
            None => Err(ComponentRejection::new(
                "binding",
                "binding reference",
                "absent",
            )),
        }
    }

    fn one_requirement(
        &self,
        value: &SemanticValue,
    ) -> Result<BindingRequirement, ComponentRejection> {
        match value {
            SemanticValue::BindingRef(binding_id) => Ok(BindingRequirement::new(
                self.binding_override.unwrap_or(binding_id),
                self.required_capability,
                self.required_element,
            )),
            other => Err(ComponentRejection::new(
                "binding",
                "binding reference",
                &format!("{other:?}"),
            )),
        }
    }
}

pub struct SyntheticCapabilityHandler {
    pub descriptor: HandlerDescriptor,
    contribution_prefix: &'static str,
    contribution_override: Option<&'static str>,
    reject_start: Option<&'static str>,
    reject_on_byte: Option<u8>,
    fail_on_byte: Option<u8>,
    reject_finish: Option<&'static str>,
    reported_identity: Option<HandlerDescriptor>,
}

impl SyntheticCapabilityHandler {
    pub fn new(handler_id: &'static str, revision: u64, contribution_prefix: &'static str) -> Self {
        Self {
            descriptor: HandlerDescriptor::new(handler_id, revision),
            contribution_prefix,
            contribution_override: None,
            reject_start: None,
            reject_on_byte: None,
            fail_on_byte: None,
            reject_finish: None,
            reported_identity: None,
        }
    }

    pub fn with_contribution_override(mut self, uri: &'static str) -> Self {
        self.contribution_override = Some(uri);
        self
    }

    pub fn with_start_rejection(mut self, reason: &'static str) -> Self {
        self.reject_start = Some(reason);
        self
    }

    pub fn with_chunk_byte_rejection(mut self, byte: u8) -> Self {
        self.reject_on_byte = Some(byte);
        self
    }

    pub fn with_chunk_byte_failure(mut self, byte: u8) -> Self {
        self.fail_on_byte = Some(byte);
        self
    }

    pub fn with_finish_rejection(mut self, reason: &'static str) -> Self {
        self.reject_finish = Some(reason);
        self
    }

    pub fn with_reported_identity(mut self, handler_id: &'static str, revision: u64) -> Self {
        self.reported_identity = Some(HandlerDescriptor::new(handler_id, revision));
        self
    }
}

impl CapabilityHandler for SyntheticCapabilityHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn start_session(
        &self,
        binding: &ResourceBinding,
    ) -> Result<Box<dyn CapabilitySession>, CapabilitySessionError> {
        if let Some(reason) = self.reject_start {
            return Err(CapabilitySessionError::Rejected {
                reason: reason.to_owned(),
            });
        }
        let payload_contract_uri = match self.contribution_override {
            Some(uri) => uri.to_owned(),
            None => format!("{}/{}", self.contribution_prefix, binding.binding_id),
        };
        let reported = self
            .reported_identity
            .clone()
            .unwrap_or_else(|| self.descriptor.clone());
        Ok(Box::new(SyntheticSession {
            descriptor: reported,
            binding_id: binding.binding_id.clone(),
            payload_contract_uri,
            reject_on_byte: self.reject_on_byte,
            fail_on_byte: self.fail_on_byte,
            reject_finish: self.reject_finish,
            bytes: 0,
        }))
    }
}

struct SyntheticSession {
    descriptor: HandlerDescriptor,
    binding_id: String,
    payload_contract_uri: String,
    reject_on_byte: Option<u8>,
    fail_on_byte: Option<u8>,
    reject_finish: Option<&'static str>,
    bytes: u64,
}

impl CapabilitySession for SyntheticSession {
    fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), CapabilitySessionError> {
        if let Some(bad) = self.reject_on_byte.filter(|bad| chunk.contains(bad)) {
            return Err(CapabilitySessionError::Rejected {
                reason: format!("synthetic byte {bad:#04x} is forbidden"),
            });
        }
        if let Some(bad) = self.fail_on_byte.filter(|bad| chunk.contains(bad)) {
            return Err(CapabilitySessionError::Failed {
                message: format!("synthetic byte {bad:#04x} broke the session"),
            });
        }
        let increment = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        self.bytes = self.bytes.saturating_add(increment);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ResourceContribution, CapabilitySessionError> {
        if let Some(reason) = self.reject_finish {
            return Err(CapabilitySessionError::Rejected {
                reason: reason.to_owned(),
            });
        }
        let contribution = map(&[
            ("origin", text(&self.descriptor.handler_id)),
            ("binding", text(&self.binding_id)),
            ("bytes", unsigned(self.bytes)),
        ]);
        Ok(ResourceContribution::new(
            self.descriptor,
            &self.payload_contract_uri,
            contribution,
        ))
    }
}

pub struct MapProvider {
    sources: HashMap<ContentAddress, Vec<u8>>,
    open_failures: HashMap<ContentAddress, String>,
    read_failures: HashSet<ContentAddress>,
    opens: RefCell<Vec<ContentAddress>>,
}

impl Default for MapProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MapProvider {
    pub fn new() -> Self {
        Self {
            sources: HashMap::new(),
            open_failures: HashMap::new(),
            read_failures: HashSet::new(),
            opens: RefCell::new(Vec::new()),
        }
    }

    pub fn with_source(self, bytes: &[u8]) -> Self {
        self.with_source_at(bytes, bytes)
    }

    pub fn with_source_at(mut self, addressed_bytes: &[u8], provided_bytes: &[u8]) -> Self {
        self.sources
            .insert(content_address(addressed_bytes), provided_bytes.to_vec());
        self
    }

    pub fn with_open_failure(mut self, addressed_bytes: &[u8], message: &str) -> Self {
        self.open_failures
            .insert(content_address(addressed_bytes), message.to_owned());
        self
    }

    pub fn with_read_failure(mut self, addressed_bytes: &[u8]) -> Self {
        self.read_failures.insert(content_address(addressed_bytes));
        self
    }

    pub fn open_count(&self) -> usize {
        self.opens.borrow().len()
    }

    pub fn opened(&self) -> Vec<ContentAddress> {
        self.opens.borrow().clone()
    }
}

impl ResourceProvider for MapProvider {
    fn open<'a>(&'a self, address: ContentAddress) -> Result<Box<dyn Read + 'a>, ProviderError> {
        self.opens.borrow_mut().push(address);
        if let Some(message) = self.open_failures.get(&address) {
            return Err(ProviderError::new(message));
        }
        if self.read_failures.contains(&address) {
            return Ok(Box::new(FailingRead));
        }
        let bytes = self.sources.get(&address).cloned().unwrap_or_default();
        Ok(Box::new(Cursor::new(bytes)))
    }
}

struct FailingRead;

impl Read for FailingRead {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "synthetic stream failure",
        ))
    }
}

pub fn alpha_handler() -> SyntheticComponentHandler {
    SyntheticComponentHandler::new("synthetic-handler/alpha", 7, ALPHA_SCHEMA, "strict-alpha")
        .with_resource_contract(BYTES_CAPABILITY, None, "synthetic:contribution/alpha")
}

pub fn beta_handler() -> SyntheticComponentHandler {
    SyntheticComponentHandler::new("synthetic-handler/beta", 3, BETA_SCHEMA, "strict-beta")
        .with_resource_contract(
            ELEMENT_CAPABILITY,
            Some(ELEMENT_LEAF),
            "synthetic:contribution/beta",
        )
}

pub fn bytes_handler() -> SyntheticCapabilityHandler {
    SyntheticCapabilityHandler::new(
        "synthetic-handler/bytes",
        11,
        "synthetic:contribution/bytes",
    )
}

pub fn element_handler() -> SyntheticCapabilityHandler {
    SyntheticCapabilityHandler::new(
        "synthetic-handler/element",
        5,
        "synthetic:contribution/element",
    )
}

pub fn standard_registry() -> RuntimeRegistry {
    registry_of(
        vec![
            (ALPHA_SCHEMA, alpha_handler()),
            (BETA_SCHEMA, beta_handler()),
        ],
        vec![
            (BYTES_CAPABILITY, bytes_handler()),
            (ELEMENT_CAPABILITY, element_handler()),
        ],
    )
}

pub fn registry_of(
    components: Vec<(&'static str, SyntheticComponentHandler)>,
    capabilities: Vec<(&'static str, SyntheticCapabilityHandler)>,
) -> RuntimeRegistry {
    let document_handler = Arc::new(SyntheticDocumentHandler::new(&components, &capabilities));
    registry_with_document_handler(components, capabilities, document_handler)
}

pub fn registry_with_document_handler(
    components: Vec<(&'static str, SyntheticComponentHandler)>,
    capabilities: Vec<(&'static str, SyntheticCapabilityHandler)>,
    document_handler: Arc<dyn DocumentHandler>,
) -> RuntimeRegistry {
    let mut builder = RuntimeRegistryBuilder::new();
    builder
        .document_schema(DOC_TAG, DOC_VERSION)
        .expect("unique schema registration")
        .document_handler(document_handler)
        .expect("unique document handler");
    for (schema_uri, handler) in components {
        builder
            .component_handler(schema_uri, Arc::new(handler))
            .expect("unique component registration");
    }
    for (capability_uri, handler) in capabilities {
        builder
            .capability_handler(capability_uri, Arc::new(handler))
            .expect("unique capability registration");
    }
    builder.build().expect("complete registry")
}

pub fn prepare(
    registry: &RuntimeRegistry,
    document: &SemanticWorldDocument,
    provider: &dyn ResourceProvider,
) -> Result<PreparedSemanticWorld, RuntimePlanError> {
    prepare_with_limits(registry, document, provider, &runtime_limits())
}

pub fn prepare_with_limits(
    registry: &RuntimeRegistry,
    document: &SemanticWorldDocument,
    provider: &dyn ResourceProvider,
    limits: &RuntimePlanLimits,
) -> Result<PreparedSemanticWorld, RuntimePlanError> {
    RuntimePlanner::new(registry).prepare(document, limits, provider)
}
