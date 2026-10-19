//! Bounded authored construction of an exact generic runtime registry.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use worldrunner_world_model::{
    ContentDigest, ResourceBinding, SchemaDescriptor, SemanticComponent, SemanticMap,
    SemanticValue, SemanticWorldDocument, validate_semantic_identifier, validate_structural_uri,
};

use crate::{
    BindingRequirement, CapabilityHandler, CapabilitySession, CapabilitySessionError,
    ComponentHandler, ComponentPlan, ComponentRejection, ContributionKind, ContributionRequirement,
    DocumentHandler, DocumentPlan, DocumentRejection, HandlerDescriptor, ResourceContribution,
    RuntimeRegistry, RuntimeRegistryBuilder, RuntimeRegistryError,
};

/// Content capability identifying one governed authored runtime profile.
pub const RUNTIME_PROFILE_CAPABILITY_URI: &str = "urn:real-disunity:capability:runtime-profile:v1";
/// Exact authored runtime-profile schema.
pub const RUNTIME_PROFILE_SCHEMA: &str = "real-disunity.runtime-profile.v1";
/// Exact authored runtime-profile version.
pub const RUNTIME_PROFILE_VERSION: u32 = 1;

/// Explicit structural limits for one authored runtime profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeProfileLimits {
    component_handlers: usize,
    capability_handlers: usize,
    roles_per_component: usize,
}

impl RuntimeProfileLimits {
    /// Creates positive explicit profile limits.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeProfileError::InvalidLimits`] if any limit is zero.
    pub fn new(
        max_component_handlers: usize,
        max_capability_handlers: usize,
        max_roles_per_component: usize,
    ) -> Result<Self, RuntimeProfileError> {
        if max_component_handlers == 0
            || max_capability_handlers == 0
            || max_roles_per_component == 0
        {
            return Err(RuntimeProfileError::InvalidLimits);
        }
        Ok(Self {
            component_handlers: max_component_handlers,
            capability_handlers: max_capability_handlers,
            roles_per_component: max_roles_per_component,
        })
    }
}

/// Strict authored description of every route in one runtime registry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthoredRuntimeProfile {
    schema: String,
    schema_version: u32,
    profile_id: String,
    world_schema: SchemaDescriptor,
    root_handler: AuthoredRootHandlerContract,
    component_handlers: Vec<AuthoredComponentHandlerContract>,
    capability_handlers: Vec<AuthoredCapabilityHandlerContract>,
}

/// Exact root-handler contract retained from a validated runtime profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthoredRootHandlerContract {
    handler_id: String,
    revision: u64,
    payload_contract_uri: String,
}

/// Exact component-handler contract retained from a validated runtime profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthoredComponentHandlerContract {
    source_schema_uri: String,
    handler_id: String,
    revision: u64,
    payload_contract_uri: String,
    roles: Vec<AuthoredRoleContract>,
}

/// Exact role-to-capability contract retained from a validated runtime profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthoredRoleContract {
    #[serde(rename = "role_uri")]
    role: String,
    #[serde(rename = "capability_uri")]
    capability: String,
    #[serde(rename = "element_uri")]
    element: Option<String>,
}

/// Exact capability-handler contract retained from a validated runtime profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AuthoredCapabilityHandlerContract {
    capability_uri: String,
    element_uri: Option<String>,
    handler_id: String,
    revision: u64,
    payload_contract_uri: String,
}

impl AuthoredRootHandlerContract {
    /// Returns the exact handler identifier.
    #[must_use]
    pub fn handler_id(&self) -> &str {
        &self.handler_id
    }

    /// Returns the exact nonzero handler revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the exact prepared-payload contract URI.
    #[must_use]
    pub fn payload_contract_uri(&self) -> &str {
        &self.payload_contract_uri
    }
}

impl AuthoredComponentHandlerContract {
    /// Returns the exact source component schema URI.
    #[must_use]
    pub fn source_schema_uri(&self) -> &str {
        &self.source_schema_uri
    }

    /// Returns the exact handler identifier.
    #[must_use]
    pub fn handler_id(&self) -> &str {
        &self.handler_id
    }

    /// Returns the exact nonzero handler revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the exact prepared-payload contract URI.
    #[must_use]
    pub fn payload_contract_uri(&self) -> &str {
        &self.payload_contract_uri
    }

    /// Returns the validated, strictly ordered role closure.
    #[must_use]
    pub fn roles(&self) -> &[AuthoredRoleContract] {
        &self.roles
    }
}

impl AuthoredRoleContract {
    /// Returns the exact semantic role URI.
    #[must_use]
    pub fn role_uri(&self) -> &str {
        &self.role
    }

    /// Returns the exact capability URI required by this role.
    #[must_use]
    pub fn capability_uri(&self) -> &str {
        &self.capability
    }

    /// Returns the exact optional element URI required by this role.
    #[must_use]
    pub fn element_uri(&self) -> Option<&str> {
        self.element.as_deref()
    }
}

impl AuthoredCapabilityHandlerContract {
    /// Returns the exact capability URI.
    #[must_use]
    pub fn capability_uri(&self) -> &str {
        &self.capability_uri
    }

    /// Returns the exact optional element URI.
    #[must_use]
    pub fn element_uri(&self) -> Option<&str> {
        self.element_uri.as_deref()
    }

    /// Returns the exact handler identifier.
    #[must_use]
    pub fn handler_id(&self) -> &str {
        &self.handler_id
    }

    /// Returns the exact nonzero handler revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the exact prepared-payload contract URI.
    #[must_use]
    pub fn payload_contract_uri(&self) -> &str {
        &self.payload_contract_uri
    }
}

/// Generic prepared component retaining exact authored roles and source semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoredComponentPayload {
    pub component_id: String,
    pub schema_uri: String,
    pub roles: BTreeMap<String, String>,
    pub source: SemanticValue,
}

/// Generic prepared resource summary produced by a single streamed session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedResourcePayload {
    pub binding_id: String,
    pub capability_uri: String,
    pub element_uri: Option<String>,
    pub content_digest: ContentDigest,
    pub byte_length: u64,
}

/// Exact document-level preparation summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoredRootPayload {
    pub profile_id: String,
    pub payload_contract_uri: String,
    pub component_count: usize,
    pub resource_count: usize,
}

impl AuthoredRuntimeProfile {
    /// Parses and validates a profile under caller-supplied structural limits.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed JSON, schema drift, invalid routes,
    /// unordered/duplicate contracts, inconsistent handler revisions, or a
    /// missing self-route for the profile capability.
    pub fn from_json(
        bytes: &[u8],
        limits: RuntimeProfileLimits,
    ) -> Result<Self, RuntimeProfileError> {
        let profile: Self = serde_json::from_slice(bytes)
            .map_err(|source| RuntimeProfileError::InvalidJson { source })?;
        profile.validate(limits)?;
        Ok(profile)
    }

    /// Returns the authored profile identifier.
    #[must_use]
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// Returns the exact world schema governed by the profile.
    #[must_use]
    pub fn world_schema(&self) -> &SchemaDescriptor {
        &self.world_schema
    }

    /// Returns the validated root-handler contract.
    #[must_use]
    pub const fn root_handler(&self) -> &AuthoredRootHandlerContract {
        &self.root_handler
    }

    /// Returns the strictly ordered component-handler contracts.
    #[must_use]
    pub fn component_handlers(&self) -> &[AuthoredComponentHandlerContract] {
        &self.component_handlers
    }

    /// Returns the strictly ordered capability-handler contracts.
    #[must_use]
    pub fn capability_handlers(&self) -> &[AuthoredCapabilityHandlerContract] {
        &self.capability_handlers
    }

    /// Finds one exact component-handler contract by source schema URI.
    #[must_use]
    pub fn component_handler(
        &self,
        source_schema_uri: &str,
    ) -> Option<&AuthoredComponentHandlerContract> {
        self.component_handlers
            .binary_search_by_key(&source_schema_uri, |contract| contract.source_schema_uri())
            .ok()
            .map(|index| &self.component_handlers[index])
    }

    /// Finds one exact capability-handler contract by capability URI.
    #[must_use]
    pub fn capability_handler(
        &self,
        capability_uri: &str,
    ) -> Option<&AuthoredCapabilityHandlerContract> {
        self.capability_handlers
            .binary_search_by_key(&capability_uri, |contract| contract.capability_uri())
            .ok()
            .map(|index| &self.capability_handlers[index])
    }

    /// Builds the immutable exact registry described by this profile.
    ///
    /// # Errors
    ///
    /// Returns a registry error if an authored route cannot be registered.
    pub fn build_registry(&self) -> Result<RuntimeRegistry, RuntimeProfileError> {
        let component_contracts = self
            .component_handlers
            .iter()
            .map(|contract| (contract.source_schema_uri.clone(), contract.clone()))
            .collect::<BTreeMap<_, _>>();
        let capability_contracts = self
            .capability_handlers
            .iter()
            .map(|contract| (contract.capability_uri.clone(), contract.clone()))
            .collect::<BTreeMap<_, _>>();
        let root = GenericDocumentHandler {
            descriptor: descriptor(&self.root_handler.handler_id, self.root_handler.revision),
            profile_id: self.profile_id.clone(),
            payload_contract_uri: self.root_handler.payload_contract_uri.clone(),
            components: component_contracts,
            capabilities: capability_contracts,
        };
        let mut builder = RuntimeRegistryBuilder::new();
        builder.document_schema(&self.world_schema.tag, self.world_schema.version)?;
        builder.document_handler(Arc::new(root))?;
        for contract in &self.component_handlers {
            builder.component_handler(
                &contract.source_schema_uri,
                Arc::new(GenericComponentHandler::new(contract)),
            )?;
        }
        for contract in &self.capability_handlers {
            builder.capability_handler(
                &contract.capability_uri,
                Arc::new(GenericCapabilityHandler::new(contract)),
            )?;
        }
        builder.build().map_err(Into::into)
    }

    fn validate(&self, limits: RuntimeProfileLimits) -> Result<(), RuntimeProfileError> {
        if self.schema != RUNTIME_PROFILE_SCHEMA || self.schema_version != RUNTIME_PROFILE_VERSION {
            return Err(RuntimeProfileError::SchemaMismatch);
        }
        identifier("profile_id", &self.profile_id)?;
        uri("world_schema.tag", &self.world_schema.tag)?;
        handler_contract(
            "root_handler",
            &self.root_handler.handler_id,
            self.root_handler.revision,
            &self.root_handler.payload_contract_uri,
        )?;
        require_limit(
            "component_handlers",
            self.component_handlers.len(),
            limits.component_handlers,
        )?;
        require_limit(
            "capability_handlers",
            self.capability_handlers.len(),
            limits.capability_handlers,
        )?;
        validate_components(&self.component_handlers, limits.roles_per_component)?;
        validate_capabilities(&self.capability_handlers)?;
        let profile_capability = self
            .capability_handlers
            .iter()
            .find(|contract| contract.capability_uri == RUNTIME_PROFILE_CAPABILITY_URI)
            .ok_or(RuntimeProfileError::MissingProfileCapability)?;
        if let Some(element_uri) = &profile_capability.element_uri {
            return Err(RuntimeProfileError::ProfileCapabilityElementMismatch {
                element_uri: element_uri.clone(),
            });
        }
        validate_role_capabilities(&self.component_handlers, &self.capability_handlers)?;
        validate_handler_revisions(self)?;
        Ok(())
    }
}

fn validate_components(
    contracts: &[AuthoredComponentHandlerContract],
    max_roles: usize,
) -> Result<(), RuntimeProfileError> {
    require_strict_order(
        "component_handlers.source_schema_uri",
        contracts
            .iter()
            .map(|contract| contract.source_schema_uri.as_str()),
    )?;
    for contract in contracts {
        uri("component.source_schema_uri", &contract.source_schema_uri)?;
        handler_contract(
            "component",
            &contract.handler_id,
            contract.revision,
            &contract.payload_contract_uri,
        )?;
        require_limit("component.roles", contract.roles.len(), max_roles)?;
        require_strict_order(
            "component.roles.role_uri",
            contract.roles.iter().map(|role| role.role.as_str()),
        )?;
        for role in &contract.roles {
            uri("component.role_uri", &role.role)?;
            uri("component.capability_uri", &role.capability)?;
            optional_uri("component.element_uri", role.element.as_deref())?;
        }
    }
    Ok(())
}

fn validate_capabilities(
    contracts: &[AuthoredCapabilityHandlerContract],
) -> Result<(), RuntimeProfileError> {
    require_strict_order(
        "capability_handlers.capability_uri",
        contracts
            .iter()
            .map(|contract| contract.capability_uri.as_str()),
    )?;
    for contract in contracts {
        uri("capability.capability_uri", &contract.capability_uri)?;
        optional_uri("capability.element_uri", contract.element_uri.as_deref())?;
        handler_contract(
            "capability",
            &contract.handler_id,
            contract.revision,
            &contract.payload_contract_uri,
        )?;
    }
    Ok(())
}

fn validate_role_capabilities(
    components: &[AuthoredComponentHandlerContract],
    capabilities: &[AuthoredCapabilityHandlerContract],
) -> Result<(), RuntimeProfileError> {
    let capabilities = capabilities
        .iter()
        .map(|contract| (contract.capability_uri.as_str(), contract))
        .collect::<BTreeMap<_, _>>();
    for component in components {
        for role in &component.roles {
            let Some(capability) = capabilities.get(role.capability.as_str()) else {
                return Err(RuntimeProfileError::MissingRoleCapability {
                    source_schema_uri: component.source_schema_uri.clone(),
                    role_uri: role.role.clone(),
                    capability_uri: role.capability.clone(),
                });
            };
            if capability.element_uri != role.element {
                return Err(RuntimeProfileError::RoleCapabilityElementMismatch {
                    source_schema_uri: component.source_schema_uri.clone(),
                    role_uri: role.role.clone(),
                    capability_uri: role.capability.clone(),
                    role_element_uri: role.element.clone(),
                    capability_element_uri: capability.element_uri.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_handler_revisions(profile: &AuthoredRuntimeProfile) -> Result<(), RuntimeProfileError> {
    let mut revisions = BTreeMap::new();
    record_authored_revision(
        &mut revisions,
        &profile.root_handler.handler_id,
        profile.root_handler.revision,
    )?;
    for contract in &profile.component_handlers {
        record_authored_revision(&mut revisions, &contract.handler_id, contract.revision)?;
    }
    for contract in &profile.capability_handlers {
        record_authored_revision(&mut revisions, &contract.handler_id, contract.revision)?;
    }
    Ok(())
}

fn record_authored_revision(
    revisions: &mut BTreeMap<String, u64>,
    handler_id: &str,
    revision: u64,
) -> Result<(), RuntimeProfileError> {
    if let Some(previous) = revisions.insert(handler_id.to_owned(), revision)
        && previous != revision
    {
        return Err(RuntimeProfileError::HandlerRevisionConflict {
            handler_id: handler_id.to_owned(),
            first: previous,
            second: revision,
        });
    }
    Ok(())
}

fn handler_contract(
    field: &'static str,
    handler_id: &str,
    revision: u64,
    payload_contract_uri: &str,
) -> Result<(), RuntimeProfileError> {
    identifier(field, handler_id)?;
    if revision == 0 {
        return Err(RuntimeProfileError::ZeroRevision {
            handler_id: handler_id.to_owned(),
        });
    }
    uri("payload_contract_uri", payload_contract_uri)
}

fn identifier(field: &'static str, value: &str) -> Result<(), RuntimeProfileError> {
    validate_semantic_identifier(value).map_err(|source| RuntimeProfileError::InvalidField {
        field,
        value: value.to_owned(),
        reason: source.to_string(),
    })
}

fn uri(field: &'static str, value: &str) -> Result<(), RuntimeProfileError> {
    validate_structural_uri(value).map_err(|source| RuntimeProfileError::InvalidField {
        field,
        value: value.to_owned(),
        reason: source.to_string(),
    })
}

fn optional_uri(field: &'static str, value: Option<&str>) -> Result<(), RuntimeProfileError> {
    value.map_or(Ok(()), |value| uri(field, value))
}

fn require_limit(
    field: &'static str,
    observed: usize,
    maximum: usize,
) -> Result<(), RuntimeProfileError> {
    if observed > maximum {
        Err(RuntimeProfileError::LimitExceeded {
            field,
            observed,
            maximum,
        })
    } else {
        Ok(())
    }
}

fn require_strict_order<'a>(
    field: &'static str,
    values: impl Iterator<Item = &'a str>,
) -> Result<(), RuntimeProfileError> {
    let mut previous: Option<&str> = None;
    for value in values {
        if previous.is_some_and(|previous| previous >= value) {
            return Err(RuntimeProfileError::InvalidOrder {
                field,
                previous: previous.unwrap_or_default().to_owned(),
                next: value.to_owned(),
            });
        }
        previous = Some(value);
    }
    Ok(())
}

fn descriptor(handler_id: &str, revision: u64) -> HandlerDescriptor {
    HandlerDescriptor::new(handler_id, revision)
}

struct GenericDocumentHandler {
    descriptor: HandlerDescriptor,
    profile_id: String,
    payload_contract_uri: String,
    components: BTreeMap<String, AuthoredComponentHandlerContract>,
    capabilities: BTreeMap<String, AuthoredCapabilityHandlerContract>,
}

impl DocumentHandler for GenericDocumentHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn plan(&self, document: &SemanticWorldDocument) -> Result<DocumentPlan, DocumentRejection> {
        let mut requirements = Vec::with_capacity(
            document
                .components()
                .len()
                .saturating_add(document.bindings().len()),
        );
        for component in document.components() {
            let contract = self.components.get(&component.schema_uri).ok_or_else(|| {
                DocumentRejection::new(
                    "components.schema_uri",
                    "an exact authored component route",
                    &component.schema_uri,
                )
            })?;
            requirements.push(ContributionRequirement::new(
                ContributionKind::Component,
                &component.component_id,
                &contract.payload_contract_uri,
            ));
        }
        for binding in document.bindings() {
            let contract = self
                .capabilities
                .get(&binding.capability_uri)
                .ok_or_else(|| {
                    DocumentRejection::new(
                        "bindings.capability_uri",
                        "an exact authored capability route",
                        &binding.capability_uri,
                    )
                })?;
            requirements.push(ContributionRequirement::new(
                ContributionKind::Resource,
                &binding.binding_id,
                &contract.payload_contract_uri,
            ));
        }
        requirements.sort();
        Ok(DocumentPlan::new(
            self.descriptor.clone(),
            AuthoredRootPayload {
                profile_id: self.profile_id.clone(),
                payload_contract_uri: self.payload_contract_uri.clone(),
                component_count: document.components().len(),
                resource_count: document.bindings().len(),
            },
            requirements,
        ))
    }
}

struct GenericComponentHandler {
    descriptor: HandlerDescriptor,
    contract: AuthoredComponentHandlerContract,
}

impl GenericComponentHandler {
    fn new(contract: &AuthoredComponentHandlerContract) -> Self {
        Self {
            descriptor: descriptor(&contract.handler_id, contract.revision),
            contract: contract.clone(),
        }
    }

    fn plan_roles(
        &self,
        roles: &SemanticMap,
    ) -> Result<(BTreeMap<String, String>, Vec<BindingRequirement>), ComponentRejection> {
        if roles.len() != self.contract.roles.len() {
            return Err(ComponentRejection::new(
                "roles",
                "exact authored role closure",
                "different role count",
            ));
        }
        let mut requirements = Vec::with_capacity(self.contract.roles.len());
        let mut prepared_roles = BTreeMap::new();
        for contract in &self.contract.roles {
            let binding_id = match roles.get(&contract.role) {
                Some(SemanticValue::BindingRef(binding_id)) => binding_id,
                Some(_) => {
                    return Err(ComponentRejection::new(
                        &contract.role,
                        "binding_ref",
                        "other",
                    ));
                }
                None => {
                    return Err(ComponentRejection::new(&contract.role, "present", "absent"));
                }
            };
            prepared_roles.insert(contract.role.clone(), binding_id.clone());
            requirements.push(BindingRequirement::new(
                binding_id,
                &contract.capability,
                contract.element.as_deref(),
            ));
        }
        requirements.sort();
        requirements.dedup();
        Ok((prepared_roles, requirements))
    }
}

impl ComponentHandler for GenericComponentHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn plan(&self, component: &SemanticComponent) -> Result<ComponentPlan, ComponentRejection> {
        if component.schema_uri != self.contract.source_schema_uri {
            return Err(ComponentRejection::new(
                "schema_uri",
                &self.contract.source_schema_uri,
                &component.schema_uri,
            ));
        }
        if component.payload.len() != 2
            || component.payload.get("roles").is_none()
            || component.payload.get("source").is_none()
        {
            return Err(ComponentRejection::new(
                "payload",
                "exactly roles and source",
                "different keys",
            ));
        }
        let Some(SemanticValue::Map(roles)) = component.payload.get("roles") else {
            return Err(ComponentRejection::new("roles", "map", "other"));
        };
        let (prepared_roles, requirements) = self.plan_roles(roles)?;
        let Some(source) = component.payload.get("source").cloned() else {
            return Err(ComponentRejection::new("source", "present", "absent"));
        };
        Ok(ComponentPlan::new(
            self.descriptor.clone(),
            &self.contract.payload_contract_uri,
            AuthoredComponentPayload {
                component_id: component.component_id.clone(),
                schema_uri: component.schema_uri.clone(),
                roles: prepared_roles,
                source,
            },
            requirements,
        ))
    }
}

struct GenericCapabilityHandler {
    descriptor: HandlerDescriptor,
    contract: AuthoredCapabilityHandlerContract,
}

impl GenericCapabilityHandler {
    fn new(contract: &AuthoredCapabilityHandlerContract) -> Self {
        Self {
            descriptor: descriptor(&contract.handler_id, contract.revision),
            contract: contract.clone(),
        }
    }
}

impl CapabilityHandler for GenericCapabilityHandler {
    fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    fn start_session(
        &self,
        binding: &ResourceBinding,
    ) -> Result<Box<dyn CapabilitySession>, CapabilitySessionError> {
        if binding.capability_uri != self.contract.capability_uri
            || binding.element_uri != self.contract.element_uri
        {
            return Err(CapabilitySessionError::Rejected {
                reason: format!(
                    "expected capability {} element {:?}, observed {} element {:?}",
                    self.contract.capability_uri,
                    self.contract.element_uri,
                    binding.capability_uri,
                    binding.element_uri
                ),
            });
        }
        Ok(Box::new(DigestCapabilitySession {
            descriptor: self.descriptor.clone(),
            payload_contract_uri: self.contract.payload_contract_uri.clone(),
            binding_id: binding.binding_id.clone(),
            capability_uri: binding.capability_uri.clone(),
            element_uri: binding.element_uri.clone(),
            hasher: Sha256::new(),
            byte_length: 0,
        }))
    }
}

struct DigestCapabilitySession {
    descriptor: HandlerDescriptor,
    payload_contract_uri: String,
    binding_id: String,
    capability_uri: String,
    element_uri: Option<String>,
    hasher: Sha256,
    byte_length: u64,
}

impl CapabilitySession for DigestCapabilitySession {
    fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), CapabilitySessionError> {
        let increment = u64::try_from(chunk.len()).map_err(|_| CapabilitySessionError::Failed {
            message: "chunk length does not fit u64".to_owned(),
        })?;
        self.byte_length = self.byte_length.checked_add(increment).ok_or_else(|| {
            CapabilitySessionError::Failed {
                message: "resource byte count overflow".to_owned(),
            }
        })?;
        self.hasher.update(chunk);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<ResourceContribution, CapabilitySessionError> {
        let digest = ContentDigest::from_bytes(self.hasher.finalize().into()).map_err(|error| {
            CapabilitySessionError::Failed {
                message: error.to_string(),
            }
        })?;
        Ok(ResourceContribution::new(
            self.descriptor,
            &self.payload_contract_uri,
            VerifiedResourcePayload {
                binding_id: self.binding_id,
                capability_uri: self.capability_uri,
                element_uri: self.element_uri,
                content_digest: digest,
                byte_length: self.byte_length,
            },
        ))
    }
}

/// Fail-closed authored runtime-profile errors.
#[derive(Debug, Error)]
pub enum RuntimeProfileError {
    #[error("runtime-profile limits must all be positive")]
    InvalidLimits,
    #[error("runtime profile JSON is invalid: {source}")]
    InvalidJson { source: serde_json::Error },
    #[error("runtime profile schema or version is unsupported")]
    SchemaMismatch,
    #[error("runtime profile field {field} value {value:?} is invalid: {reason}")]
    InvalidField {
        field: &'static str,
        value: String,
        reason: String,
    },
    #[error("runtime profile {field} has {observed} entries, maximum {maximum}")]
    LimitExceeded {
        field: &'static str,
        observed: usize,
        maximum: usize,
    },
    #[error("runtime profile {field} is not strictly ordered: {previous:?} then {next:?}")]
    InvalidOrder {
        field: &'static str,
        previous: String,
        next: String,
    },
    #[error("runtime handler {handler_id:?} has zero revision")]
    ZeroRevision { handler_id: String },
    #[error("runtime handler {handler_id:?} has conflicting revisions {first} and {second}")]
    HandlerRevisionConflict {
        handler_id: String,
        first: u64,
        second: u64,
    },
    #[error(
        "component schema {source_schema_uri:?} role {role_uri:?} references unhandled capability {capability_uri:?}"
    )]
    MissingRoleCapability {
        source_schema_uri: String,
        role_uri: String,
        capability_uri: String,
    },
    #[error(
        "component schema {source_schema_uri:?} role {role_uri:?} capability {capability_uri:?} element {role_element_uri:?} differs from capability route element {capability_element_uri:?}"
    )]
    RoleCapabilityElementMismatch {
        source_schema_uri: String,
        role_uri: String,
        capability_uri: String,
        role_element_uri: Option<String>,
        capability_element_uri: Option<String>,
    },
    #[error("runtime profile does not route its own governed capability")]
    MissingProfileCapability,
    #[error(
        "runtime profile routes its own governed capability through typed element {element_uri:?} instead of the required bare binding"
    )]
    ProfileCapabilityElementMismatch { element_uri: String },
    #[error("runtime profile registry construction failed: {0}")]
    Registry(#[from] RuntimeRegistryError),
}
