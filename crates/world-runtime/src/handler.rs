//! Object-safe negotiation contracts for components and capabilities.

use thiserror::Error;
use worldrunner_world_model::{ResourceBinding, SemanticComponent};

use crate::payload::{PreparedPayload, RuntimePayload};

/// Deterministic identity and revision of one handler implementation.
///
/// Plans and contributions carry the descriptor of the handler that produced
/// them so downstream consumers can detect negotiation drift exactly.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandlerDescriptor {
    /// Stable identity of the handler implementation.
    pub handler_id: String,
    /// Revision of the handler implementation's negotiation behaviour.
    pub revision: u64,
}

impl HandlerDescriptor {
    /// Creates a descriptor from an identity and revision.
    pub fn new(handler_id: &str, revision: u64) -> Self {
        Self {
            handler_id: handler_id.to_owned(),
            revision,
        }
    }
}

/// Typed rejection of one component payload by a component handler.
///
/// The three fields preserve exactly what the handler's contract checked so
/// callers can diagnose mismatches without re-deriving payload state.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("component handler rejected field {field:?}: expected {expected:?}, observed {observed:?}")]
pub struct ComponentRejection {
    /// Field of the payload that violated the handler contract.
    pub field: String,
    /// The value the handler contract requires.
    pub expected: String,
    /// The value observed in the payload.
    pub observed: String,
}

impl ComponentRejection {
    /// Creates a rejection from the checked field and both spellings.
    pub fn new(field: &str, expected: &str, observed: &str) -> Self {
        Self {
            field: field.to_owned(),
            expected: expected.to_owned(),
            observed: observed.to_owned(),
        }
    }
}

/// One component's exact demand on one present resource binding.
///
/// Ordering is the derived field order, so requirements sort ascending by
/// binding identifier, then capability URI, then element URI.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingRequirement {
    /// Exact identifier of the demanded binding.
    pub binding_id: String,
    /// Exact capability URI the demanded binding must serve.
    pub capability_uri: String,
    /// Exact optional element URI the demanded binding must carry.
    pub element_uri: Option<String>,
}

impl BindingRequirement {
    /// Creates a requirement naming one exact binding, capability, and element.
    pub fn new(binding_id: &str, capability_uri: &str, element_uri: Option<&str>) -> Self {
        Self {
            binding_id: binding_id.to_owned(),
            capability_uri: capability_uri.to_owned(),
            element_uri: element_uri.map(str::to_owned),
        }
    }
}
/// Origin domain of one contribution required by the document root contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContributionKind {
    /// A contribution produced from one semantic component.
    Component,
    /// A contribution produced from one streamed resource binding.
    Resource,
}

/// One exact contribution required by a document root contract.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContributionRequirement {
    /// Whether the claimant is a component or resource binding.
    pub kind: ContributionKind,
    /// Exact component or binding identifier that must produce the contribution.
    pub claimant: String,
    /// Exact URI identifying the required prepared payload contract.
    ///
    /// This identifies a payload shape, not a claimant instance, so distinct
    /// claimants may intentionally use the same contract URI.
    pub payload_contract_uri: String,
}

impl ContributionRequirement {
    /// Creates one exact root contribution requirement.
    pub fn new(kind: ContributionKind, claimant: &str, payload_contract_uri: &str) -> Self {
        Self {
            kind,
            claimant: claimant.to_owned(),
            payload_contract_uri: payload_contract_uri.to_owned(),
        }
    }
}

/// Typed rejection of a complete world document by its root handler.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("document handler rejected field {field:?}: expected {expected:?}, observed {observed:?}")]
pub struct DocumentRejection {
    /// Field of the document that violated the root contract.
    pub field: String,
    /// Value required by the root contract.
    pub expected: String,
    /// Value observed in the document.
    pub observed: String,
}

impl DocumentRejection {
    /// Creates a rejection from the checked field and both spellings.
    pub fn new(field: &str, expected: &str, observed: &str) -> Self {
        Self {
            field: field.to_owned(),
            expected: expected.to_owned(),
            observed: observed.to_owned(),
        }
    }
}

/// Exact root plan for one successfully validated world document.
#[derive(Clone, Debug)]
pub struct DocumentPlan {
    /// Descriptor of the root handler that produced this plan.
    pub handler: HandlerDescriptor,
    /// Immutable typed payload for the exact document-level contract.
    pub payload: PreparedPayload,
    /// Sorted unique closure of contributions required by the root contract.
    pub contribution_requirements: Vec<ContributionRequirement>,
}

impl DocumentPlan {
    /// Adopts root plan parts; requirement ordering is checked by the planner.
    pub fn new<T>(
        handler: HandlerDescriptor,
        payload: T,
        contribution_requirements: Vec<ContributionRequirement>,
    ) -> Self
    where
        T: RuntimePayload,
    {
        Self {
            handler,
            payload: PreparedPayload::new(payload),
            contribution_requirements,
        }
    }
}

/// Strictly validates the complete document and declares its exact output closure.
pub trait DocumentHandler: Send + Sync {
    /// Returns the deterministic identity and revision of this handler.
    fn descriptor(&self) -> &HandlerDescriptor;

    /// Validates the complete document and returns its exact root plan.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentRejection`] when any root-level contract is violated.
    fn plan(
        &self,
        document: &worldrunner_world_model::SemanticWorldDocument,
    ) -> Result<DocumentPlan, DocumentRejection>;
}

/// Result of one successful component negotiation.
#[derive(Clone, Debug)]
pub struct ComponentPlan {
    /// Descriptor of the handler that produced this plan.
    pub handler: HandlerDescriptor,
    /// URI of the contribution this component makes to the prepared world.
    pub payload_contract_uri: String,
    /// Immutable renderer-ready payload for the exact contribution contract.
    pub payload: PreparedPayload,
    /// Sorted unique binding demands of this component.
    pub binding_requirements: Vec<BindingRequirement>,
}

impl ComponentPlan {
    /// Adopts the plan parts; ordering of requirements is the planner's to check.
    pub fn new<T>(
        handler: HandlerDescriptor,
        payload_contract_uri: &str,
        payload: T,
        binding_requirements: Vec<BindingRequirement>,
    ) -> Self
    where
        T: RuntimePayload,
    {
        Self {
            handler,
            payload_contract_uri: payload_contract_uri.to_owned(),
            payload: PreparedPayload::new(payload),
            binding_requirements,
        }
    }
}

/// Negotiates components whose schema URI routes to this handler.
///
/// Implementations receive the complete component and must strictly validate
/// its full payload themselves; the runtime performs no permissive checks on
/// their behalf.
pub trait ComponentHandler: Send + Sync {
    /// Returns the deterministic identity and revision of this handler.
    fn descriptor(&self) -> &HandlerDescriptor;

    /// Strictly validates the complete component and returns its plan.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentRejection`] naming the offending field, the expected
    /// spelling, and the observed spelling whenever the payload violates this
    /// handler's contract.
    fn plan(&self, component: &SemanticComponent) -> Result<ComponentPlan, ComponentRejection>;
}

/// Typed failures a capability session can raise.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapabilitySessionError {
    /// The streamed content violates the capability's contract.
    #[error("capability session rejected content: {reason:?}")]
    Rejected {
        /// Description of the violated contract.
        reason: String,
    },
    /// The session failed operationally while consuming content.
    #[error("capability session failed: {message:?}")]
    Failed {
        /// Description of the operational failure.
        message: String,
    },
}

/// Result of one successful capability session for a single binding.
#[derive(Clone, Debug)]
pub struct ResourceContribution {
    /// Descriptor of the handler whose session produced this contribution.
    pub handler: HandlerDescriptor,
    /// URI of the contribution this resource makes to the prepared world.
    pub payload_contract_uri: String,
    /// Immutable renderer-ready payload for the exact contribution contract.
    pub payload: PreparedPayload,
}

impl ResourceContribution {
    /// Adopts the contribution parts.
    pub fn new<T>(handler: HandlerDescriptor, payload_contract_uri: &str, payload: T) -> Self
    where
        T: RuntimePayload,
    {
        Self {
            handler,
            payload_contract_uri: payload_contract_uri.to_owned(),
            payload: PreparedPayload::new(payload),
        }
    }
}

/// Stateful consumer of one binding's content stream.
///
/// Chunks arrive contiguously and exactly once per byte; the session observes
/// the whole content before [`CapabilitySession::finish`] is called.
pub trait CapabilitySession: Send {
    /// Receives the next contiguous chunk of the binding's content.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilitySessionError`] when the chunk violates the
    /// capability contract or the session fails operationally.
    fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), CapabilitySessionError>;

    /// Finishes the session and returns the binding's contribution.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilitySessionError`] when the accumulated content
    /// violates the capability contract or the session failed.
    fn finish(self: Box<Self>) -> Result<ResourceContribution, CapabilitySessionError>;
}

/// Negotiates bindings whose capability URI routes to this handler.
///
/// Handlers are selected only by exact capability URI and always receive the
/// complete [`ResourceBinding`]; they never see or derive a path.
pub trait CapabilityHandler: Send + Sync {
    /// Returns the deterministic identity and revision of this handler.
    fn descriptor(&self) -> &HandlerDescriptor;

    /// Starts a stateful session for the complete binding.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilitySessionError`] when the binding itself violates
    /// the capability contract or a session cannot be started.
    fn start_session(
        &self,
        binding: &ResourceBinding,
    ) -> Result<Box<dyn CapabilitySession>, CapabilitySessionError>;
}
