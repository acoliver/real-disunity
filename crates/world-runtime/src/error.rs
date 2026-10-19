//! Typed failures for registry construction and world preparation.

use std::io;

use thiserror::Error;
use worldrunner_world_model::{ContentDigest, ResourceBinding, SchemaDescriptor, WorldModelError};

use crate::handler::{
    BindingRequirement, CapabilitySessionError, ComponentRejection, ContributionRequirement,
    DocumentRejection, HandlerDescriptor,
};
use crate::registry::HandlerKind;
use crate::resource::ProviderError;

/// Failures that can occur while building a [`crate::RuntimeRegistry`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuntimeRegistryError {
    /// Two handlers were registered for one exact routing key.
    #[error("duplicate {handler_kind:?} handler registration for {key:?}")]
    DuplicateHandler {
        /// The kind of handler that collided.
        handler_kind: HandlerKind,
        /// The exact key that already carried a handler.
        key: String,
    },
    /// The builder finished without the mandatory strict document-root handler.
    #[error("exactly one document-root handler must be registered before build")]
    MissingDocumentHandler,
    /// The builder finished without the one required document schema.
    #[error("exactly one document schema must be registered before build")]
    MissingDocumentSchema,
    /// A handler descriptor carried an unusable identity.
    #[error("handler identity must be nonempty and free of control characters: {handler_id:?}")]
    InvalidHandlerDescriptor {
        /// The rejected handler identity.
        handler_id: String,
    },
    /// A registered schema or capability route is not a structural URI.
    #[error("invalid {handler_kind:?} routing key {key:?}: {source}")]
    InvalidRoutingKey {
        /// The kind of route carrying the malformed key.
        handler_kind: HandlerKind,
        /// The malformed route key.
        key: String,
        /// Universal structural validation failure.
        source: WorldModelError,
    },
}

/// Failures that can occur while preparing a semantic world document.
///
/// Every variant describes one exact fail-closed refusal; bulky payloads are
/// boxed so the error stays cheap to propagate.
#[derive(Debug, Error)]
pub enum RuntimePlanError {
    /// The document carries more components than the caller permits.
    #[error("document carries {observed} components but the caller permits at most {max}")]
    ComponentLimitExceeded { max: usize, observed: usize },
    /// The document carries more bindings than the caller permits.
    #[error("document carries {observed} bindings but the caller permits at most {max}")]
    BindingLimitExceeded { max: usize, observed: usize },
    /// One binding declares more bytes than the caller permits.
    #[error(
        "binding {binding_id:?} declares {observed} bytes but the caller permits at most {max}"
    )]
    BindingByteLimitExceeded {
        binding_id: String,
        max: u64,
        observed: u64,
    },
    /// The aggregate declared resource bytes exceed the caller's budget.
    #[error("document bindings declare {observed} bytes but the caller permits at most {max}")]
    TotalBindingByteLimitExceeded { max: u64, observed: u64 },
    /// The document schema is not the one exact registered schema.
    #[error(
        "document schema does not match the registered schema: expected {expected:?}, observed {observed:?}"
    )]
    UnsupportedDocumentSchema {
        /// The one schema the registry accepts.
        expected: SchemaDescriptor,
        /// The schema the document carries.
        observed: SchemaDescriptor,
    },
    /// The strict document-root handler rejected the complete document.
    #[error("document handler {handler_id:?} rejected the root contract: {rejection}")]
    DocumentRejected {
        /// Identity of the rejecting root handler.
        handler_id: String,
        /// Typed root-level rejection.
        rejection: Box<DocumentRejection>,
    },
    /// Root contribution requirements were not in strict ascending order.
    #[error("document contribution requirements are not ordered: {previous:?} before {next:?}")]
    InvalidContributionRequirementOrder {
        /// Requirement that sorted after its successor.
        previous: Box<ContributionRequirement>,
        /// Requirement that sorted before its predecessor.
        next: Box<ContributionRequirement>,
    },
    /// The root plan listed one exact contribution requirement twice.
    #[error("document contribution requirement is duplicated: {requirement:?}")]
    DuplicateContributionRequirement {
        /// Duplicated requirement.
        requirement: Box<ContributionRequirement>,
    },
    /// A root-required contribution was not produced exactly.
    #[error("required document contribution was not produced: {requirement:?}")]
    MissingContribution {
        /// Exact absent requirement.
        requirement: Box<ContributionRequirement>,
    },
    /// A handler produced a contribution outside the root's exact closure.
    #[error("prepared contribution is outside the document root closure: {contribution:?}")]
    UnexpectedContribution {
        /// Exact unexpected contribution.
        contribution: Box<ContributionRequirement>,
    },
    /// No component handler is registered for the component's exact schema URI.
    #[error("no component handler for schema {schema_uri:?} of component {component_id:?}")]
    UnsupportedComponentSchema {
        /// Identifier of the component that failed to route.
        component_id: String,
        /// The exact schema URI that carried no handler.
        schema_uri: String,
    },
    /// No capability handler is registered for the binding's exact capability URI.
    #[error("no capability handler for capability {capability_uri:?} of binding {binding_id:?}")]
    UnsupportedCapability {
        /// Identifier of the binding that failed to route.
        binding_id: String,
        /// The exact capability URI that carried no handler.
        capability_uri: String,
    },
    /// A component handler rejected the complete component payload.
    #[error(
        "handler {handler_id:?} rejected component {component_id:?} with schema {schema_uri:?}: {rejection}"
    )]
    ComponentRejected {
        /// Identifier of the rejected component.
        component_id: String,
        /// Schema URI the component claimed.
        schema_uri: String,
        /// Identity of the rejecting handler.
        handler_id: String,
        /// The typed rejection preserving field, expected, and observed.
        rejection: Box<ComponentRejection>,
    },
    /// A reported handler identity does not match its registered descriptor.
    #[error(
        "handler identity drift for claimant {claimant:?}: registered {registered:?}, reported {reported:?}"
    )]
    HandlerIdentityMismatch {
        /// The component or binding whose negotiation reported the drift.
        claimant: String,
        /// The descriptor the registry holds.
        registered: HandlerDescriptor,
        /// The descriptor reported by the plan or session.
        reported: HandlerDescriptor,
    },
    /// A handler produced a malformed identifier or structural URI.
    #[error("handler output for claimant {claimant:?} has invalid {field} {value:?}: {source}")]
    InvalidHandlerOutput {
        /// Component or binding whose handler produced the output.
        claimant: String,
        /// Name of the malformed output field.
        field: &'static str,
        /// Malformed output value.
        value: String,
        /// Universal structural validation failure.
        source: WorldModelError,
    },
    /// A component plan listed binding requirements out of ascending order.
    #[error(
        "binding requirements of component {component_id:?} are not in ascending order: {previous:?} before {next:?}"
    )]
    InvalidRequirementOrder {
        /// Identifier of the component whose plan is misordered.
        component_id: String,
        /// The requirement that sorted after its successor.
        previous: Box<BindingRequirement>,
        /// The requirement that sorted before its predecessor.
        next: Box<BindingRequirement>,
    },
    /// A component plan listed one binding requirement twice.
    #[error("binding requirement duplicated by component {component_id:?}: {requirement:?}")]
    DuplicateRequirement {
        /// Identifier of the component whose plan repeats a requirement.
        component_id: String,
        /// The duplicated requirement.
        requirement: Box<BindingRequirement>,
    },
    /// A binding requirement named a binding that the document does not carry.
    #[error("binding requirement of component {component_id:?} does not resolve: {binding_id:?}")]
    UnresolvedBindingRequirement {
        /// Identifier of the component whose requirement failed to resolve.
        component_id: String,
        /// The unresolved binding identifier.
        binding_id: String,
    },
    /// A resolved binding serves a different capability than the requirement demands.
    #[error(
        "binding {binding_id:?} required with capability {required:?} but carries {observed:?}"
    )]
    BindingCapabilityMismatch {
        /// Identifier of the component whose requirement mismatched.
        component_id: String,
        /// Identifier of the mismatched binding.
        binding_id: String,
        /// The exact capability URI the requirement demands.
        required: String,
        /// The exact capability URI the binding carries.
        observed: String,
    },
    /// A resolved binding carries a different element URI than the requirement demands.
    #[error("binding {binding_id:?} required with element {required:?} but carries {observed:?}")]
    BindingElementMismatch {
        /// Identifier of the component whose requirement mismatched.
        component_id: String,
        /// Identifier of the mismatched binding.
        binding_id: String,
        /// The exact optional element URI the requirement demands.
        required: Option<String>,
        /// The exact optional element URI the binding carries.
        observed: Option<String>,
    },
    /// A present binding was never claimed by any component requirement.
    #[error("binding {binding_id:?} with capability {capability_uri:?} is present but unconsumed")]
    UnconsumedBinding {
        /// Identifier of the unclaimed binding.
        binding_id: String,
        /// Capability URI of the unclaimed binding.
        capability_uri: String,
    },
    /// The resource provider refused to open a binding's content.
    #[error(
        "provider refused to open binding {binding_id:?} with capability {capability_uri:?}: {source}"
    )]
    ResourceOpen {
        /// Identifier of the binding whose content was refused.
        binding_id: String,
        /// Capability URI of the refused binding.
        capability_uri: String,
        /// The provider's typed failure.
        source: ProviderError,
    },
    /// Reading a binding's content stream failed.
    #[error("reading content of binding {binding_id:?} failed: {source}")]
    ResourceRead {
        /// Identifier of the binding whose stream failed.
        binding_id: String,
        /// The underlying I/O failure.
        source: io::Error,
    },
    /// The streamed byte count differs from the binding's declared length.
    #[error("binding {binding_id:?} streamed {observed} bytes but declares {expected}")]
    ResourceLengthMismatch {
        /// Identifier of the binding whose length drifted.
        binding_id: String,
        /// The declared byte length.
        expected: u64,
        /// The verified byte count.
        observed: u64,
    },
    /// The streamed content hash differs from the binding's declared digest.
    #[error("binding {binding_id:?} streamed digest {observed:?} but declares {expected:?}")]
    ResourceDigestMismatch {
        /// Identifier of the binding whose digest drifted.
        binding_id: String,
        /// The declared content digest.
        expected: ContentDigest,
        /// The verified content digest.
        observed: ContentDigest,
    },
    /// A capability session rejected the binding's content against its contract.
    #[error(
        "handler {handler:?} rejected content of binding {binding_id:?} with capability {capability_uri:?}: {reason:?}"
    )]
    SessionRejected {
        /// Identifier of the rejected binding.
        binding_id: String,
        /// Capability URI of the rejected binding.
        capability_uri: String,
        /// Descriptor of the rejecting handler.
        handler: HandlerDescriptor,
        /// The session's rejection reason.
        reason: String,
    },
    /// A capability session failed operationally while consuming content.
    #[error(
        "handler {handler:?} failed on binding {binding_id:?} with capability {capability_uri:?}: {message:?}"
    )]
    SessionFailed {
        /// Identifier of the failed binding.
        binding_id: String,
        /// Capability URI of the failed binding.
        capability_uri: String,
        /// Descriptor of the failing handler.
        handler: HandlerDescriptor,
        /// The session's failure description.
        message: String,
    },
    /// A checked byte counter exceeded its representable range.
    #[error("resource byte counter exceeded its representable range")]
    CounterOverflow,
    /// The document failed world-model validation under the caller's limits.
    #[error(transparent)]
    WorldModel(#[from] WorldModelError),
}

impl RuntimePlanError {
    /// Wraps a capability session failure with full binding context.
    pub(crate) fn from_session(
        binding: &ResourceBinding,
        handler: &HandlerDescriptor,
        error: CapabilitySessionError,
    ) -> Self {
        match error {
            CapabilitySessionError::Rejected { reason } => Self::SessionRejected {
                binding_id: binding.binding_id.clone(),
                capability_uri: binding.capability_uri.clone(),
                handler: handler.clone(),
                reason,
            },
            CapabilitySessionError::Failed { message } => Self::SessionFailed {
                binding_id: binding.binding_id.clone(),
                capability_uri: binding.capability_uri.clone(),
                handler: handler.clone(),
                message,
            },
        }
    }
}

/// Rebuilds a component rejection into its planner-level shape.
///
/// Kept next to the error enum so handler contracts stay free of planner
/// context; the planner supplies component, schema, and handler identity.
pub(crate) fn component_rejection(
    component_id: &str,
    schema_uri: &str,
    handler: &HandlerDescriptor,
    rejection: ComponentRejection,
) -> RuntimePlanError {
    RuntimePlanError::ComponentRejected {
        component_id: component_id.to_owned(),
        schema_uri: schema_uri.to_owned(),
        handler_id: handler.handler_id.clone(),
        rejection: Box::new(rejection),
    }
}
