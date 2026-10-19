//! Authored route declarations for the externally authored render contract.
//!
//! These declarations are data only. This module never constructs handlers,
//! sessions, or runtime routes, and it hardcodes no known factory, role,
//! schema, capability, or payload identities: every URI and identifier is
//! authored externally and validated only against the universal structural
//! and semantic rules. A later registry phase must match each declaration's
//! exact authored handler factory, handler identity, and payload contract,
//! and a route naming a factory or role unknown to that registry fails
//! closed there; this layer neither invents nor accepts substitutes. Route
//! roles and asset binding roles are reconciled against the authoritative
//! publication resource bindings in that later phase.

use serde::{Deserialize, Serialize};

use crate::document::error::ContractDocumentError;
use crate::document::validate::{
    require_identifier, require_positive_u32, require_positive_u64, require_sorted_unique,
    require_uri,
};

/// Authored cardinality bounds for one component or capability route.
///
/// The maximum is strictly positive so a declared route can always carry at
/// least one element; a zero minimum is valid and declares an optional route.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteCardinality {
    minimum: u64,
    maximum: u64,
}

impl RouteCardinality {
    /// Validates and adopts one cardinality bound pair.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the maximum is zero or the
    /// minimum exceeds the maximum.
    pub fn new(minimum: u64, maximum: u64) -> Result<Self, ContractDocumentError> {
        let cardinality = Self { minimum, maximum };
        cardinality.validate()?;
        Ok(cardinality)
    }

    /// Returns the authored minimum element count.
    #[must_use]
    pub const fn minimum(&self) -> u64 {
        self.minimum
    }

    /// Returns the authored maximum element count.
    #[must_use]
    pub const fn maximum(&self) -> u64 {
        self.maximum
    }

    /// Validates the bound pair.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when the maximum is zero or the
    /// minimum exceeds the maximum.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_positive_u64("route cardinality maximum", self.maximum)?;
        if self.minimum > self.maximum {
            return Err(ContractDocumentError::ViolatedInvariant {
                field: "route cardinality",
                reason: "minimum must not exceed maximum",
            });
        }
        Ok(())
    }
}

/// Shared authored handler identity of one route declaration.
///
/// Every route declaration carries the same exact authored handler factory
/// URI, semantic handler identifier, handler revision, payload contract URI,
/// and role URI. The identity is data only: a later registry must match
/// these exact spellings or fail closed. Declarations flatten this identity
/// into their own object, so the authored JSON stays one flat declaration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteIdentity {
    handler_factory_uri: String,
    handler_id: String,
    handler_revision: u64,
    payload_contract_uri: String,
    role_uri: String,
}

impl RouteIdentity {
    /// Validates and adopts one authored route identity.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn new(
        handler_factory_uri: String,
        handler_id: String,
        handler_revision: u64,
        payload_contract_uri: String,
        role_uri: String,
    ) -> Result<Self, ContractDocumentError> {
        let identity = Self {
            handler_factory_uri,
            handler_id,
            handler_revision,
            payload_contract_uri,
            role_uri,
        };
        identity.validate()?;
        Ok(identity)
    }

    /// Returns the authored handler factory URI.
    #[must_use]
    pub fn handler_factory_uri(&self) -> &str {
        &self.handler_factory_uri
    }

    /// Returns the authored semantic handler identifier.
    #[must_use]
    pub fn handler_id(&self) -> &str {
        &self.handler_id
    }

    /// Returns the authored handler revision; zero is a valid revision.
    #[must_use]
    pub const fn handler_revision(&self) -> u64 {
        self.handler_revision
    }

    /// Returns the authored payload contract URI.
    #[must_use]
    pub fn payload_contract_uri(&self) -> &str {
        &self.payload_contract_uri
    }

    /// Returns the authored role URI.
    #[must_use]
    pub fn role_uri(&self) -> &str {
        &self.role_uri
    }

    /// Validates every authored identity field.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_uri("route handler_factory_uri", &self.handler_factory_uri)?;
        require_identifier("route handler_id", &self.handler_id)?;
        require_uri("route payload_contract_uri", &self.payload_contract_uri)?;
        require_uri("route role_uri", &self.role_uri)
    }
}

/// Authored element policy one route serves.
///
/// The policy is authored data only: `Any` explicitly accepts both bare and
/// element-qualified bindings, `Bare` accepts only bindings carrying no
/// element URI, and `Exact` accepts one exact element URI. Unknown variants
/// and unknown fields fail closed at decode time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
// `Bare` is an empty struct variant, not a unit variant: serde's internally
// tagged unit-variant path silently ignores unknown fields, while the struct
// variant path honors `deny_unknown_fields`.
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingElementPolicy {
    /// The route explicitly accepts both bare and element-qualified bindings.
    Any {},
    /// The route serves bindings that carry no element URI.
    Bare {},
    /// The route serves bindings that carry exactly `element_uri`.
    Exact {
        /// The one exact structural element URI the route serves.
        element_uri: String,
    },
}

impl BindingElementPolicy {
    /// Validates the exact element URI of the `Exact` variant.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when an `Exact` policy carries a
    /// malformed element URI.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        match self {
            Self::Any {} | Self::Bare {} => Ok(()),
            Self::Exact { element_uri } => require_uri("element policy element_uri", element_uri),
        }
    }
}

/// Authored route for the document as a whole.
///
/// The declaration is data only: the schema URI and nonzero schema version
/// identify the document schema this route serves, and the flattened
/// [`RouteIdentity`] names the exact authored handler and role a later
/// registry must match or fail closed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentRouteDeclaration {
    schema_uri: String,
    schema_version: u32,
    #[serde(flatten)]
    identity: RouteIdentity,
}

impl DocumentRouteDeclaration {
    /// Validates and adopts one document route declaration.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn new(
        schema_uri: String,
        schema_version: u32,
        identity: RouteIdentity,
    ) -> Result<Self, ContractDocumentError> {
        let route = Self {
            schema_uri,
            schema_version,
            identity,
        };
        route.validate()?;
        Ok(route)
    }

    /// Returns the authored document schema URI.
    #[must_use]
    pub fn schema_uri(&self) -> &str {
        &self.schema_uri
    }

    /// Returns the nonzero authored document schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Returns the authored handler identity.
    #[must_use]
    pub const fn identity(&self) -> &RouteIdentity {
        &self.identity
    }

    /// Validates the schema reference and the handler identity.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_uri("document route schema_uri", &self.schema_uri)?;
        require_positive_u32("document route schema_version", self.schema_version)?;
        self.identity.validate()
    }
}

/// Authored route for one component schema.
///
/// The declaration is data only and carries the authored component schema
/// URI, the flattened [`RouteIdentity`], and the cardinality bounds the
/// schema's component population must respect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentRouteDeclaration {
    schema_uri: String,
    #[serde(flatten)]
    identity: RouteIdentity,
    cardinality: RouteCardinality,
}

impl ComponentRouteDeclaration {
    /// Validates and adopts one component route declaration.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn new(
        schema_uri: String,
        identity: RouteIdentity,
        cardinality: RouteCardinality,
    ) -> Result<Self, ContractDocumentError> {
        let route = Self {
            schema_uri,
            identity,
            cardinality,
        };
        route.validate()?;
        Ok(route)
    }

    /// Returns the authored component schema URI.
    #[must_use]
    pub fn schema_uri(&self) -> &str {
        &self.schema_uri
    }

    /// Returns the authored handler identity.
    #[must_use]
    pub const fn identity(&self) -> &RouteIdentity {
        &self.identity
    }

    /// Returns the authored cardinality bounds.
    #[must_use]
    pub const fn cardinality(&self) -> RouteCardinality {
        self.cardinality
    }

    /// Validates the schema key, handler identity, and cardinality bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_uri("component route schema_uri", &self.schema_uri)?;
        self.identity.validate()?;
        self.cardinality.validate()
    }
}

/// Authored route for one capability.
///
/// The declaration is data only and carries the authored capability URI it
/// is keyed by, the flattened [`RouteIdentity`], the cardinality bounds, and
/// the mandatory element policy the bound content must satisfy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRouteDeclaration {
    capability_uri: String,
    #[serde(flatten)]
    identity: RouteIdentity,
    cardinality: RouteCardinality,
    element_policy: BindingElementPolicy,
}

impl CapabilityRouteDeclaration {
    /// Validates and adopts one capability route declaration.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn new(
        capability_uri: String,
        identity: RouteIdentity,
        cardinality: RouteCardinality,
        element_policy: BindingElementPolicy,
    ) -> Result<Self, ContractDocumentError> {
        let route = Self {
            capability_uri,
            identity,
            cardinality,
            element_policy,
        };
        route.validate()?;
        Ok(route)
    }

    /// Returns the authored capability URI keying this route.
    #[must_use]
    pub fn capability_uri(&self) -> &str {
        &self.capability_uri
    }

    /// Returns the authored handler identity.
    #[must_use]
    pub const fn identity(&self) -> &RouteIdentity {
        &self.identity
    }

    /// Returns the authored cardinality bounds.
    #[must_use]
    pub const fn cardinality(&self) -> RouteCardinality {
        self.cardinality
    }

    /// Returns the authored element policy.
    #[must_use]
    pub const fn element_policy(&self) -> &BindingElementPolicy {
        &self.element_policy
    }

    /// Validates the capability key, identity, cardinality, and policy.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_uri("capability route capability_uri", &self.capability_uri)?;
        self.identity.validate()?;
        self.cardinality.validate()?;
        self.element_policy.validate()
    }
}

/// Authored route declarations of one render contract.
///
/// Exactly one document route is declared. Component routes are keyed by
/// their schema URI and capability routes by their capability URI, each in
/// strictly ascending duplicate-free order; both lists may be empty at this
/// pure schema level. The cross-part contract validation requires a
/// nonempty capability list whenever asset bindings are present.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDeclarations {
    document: DocumentRouteDeclaration,
    components: Vec<ComponentRouteDeclaration>,
    capabilities: Vec<CapabilityRouteDeclaration>,
}

impl RouteDeclarations {
    /// Validates and adopts the document route and both ordered lists.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when any route is malformed or a
    /// component or capability list is unsorted or carries a duplicated key.
    pub fn new(
        document: DocumentRouteDeclaration,
        components: Vec<ComponentRouteDeclaration>,
        capabilities: Vec<CapabilityRouteDeclaration>,
    ) -> Result<Self, ContractDocumentError> {
        let routes = Self {
            document,
            components,
            capabilities,
        };
        routes.validate()?;
        Ok(routes)
    }

    /// Returns the mandatory document route.
    #[must_use]
    pub const fn document(&self) -> &DocumentRouteDeclaration {
        &self.document
    }

    /// Returns the strictly ordered component routes.
    #[must_use]
    pub fn components(&self) -> &[ComponentRouteDeclaration] {
        &self.components
    }

    /// Returns the strictly ordered capability routes.
    #[must_use]
    pub fn capabilities(&self) -> &[CapabilityRouteDeclaration] {
        &self.capabilities
    }

    /// Validates the document route and both ordered keyed lists.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        self.document.validate()?;
        require_sorted_unique(
            "route components",
            self.components
                .iter()
                .map(|route| route.schema_uri.as_str()),
        )?;
        for route in &self.components {
            route.validate()?;
        }
        require_sorted_unique(
            "route capabilities",
            self.capabilities
                .iter()
                .map(|route| route.capability_uri.as_str()),
        )?;
        for route in &self.capabilities {
            route.validate()?;
        }
        Ok(())
    }
}
