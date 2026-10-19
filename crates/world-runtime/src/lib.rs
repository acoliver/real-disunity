//! Fail-closed runtime negotiation for semantic world documents.
//!
//! This crate is the pure negotiation foundation between validated
//! [`SemanticWorldDocument`]s and runtime consumers: a caller-built immutable
//! [`RuntimeRegistry`] routes components by exact schema URI and resources by
//! exact capability URI, a [`RuntimePlanner`] resolves every handler before
//! opening any content, streams each present binding exactly once through one
//! fixed buffer while verifying its content address, and returns an opaque
//! [`PreparedSemanticWorld`] only after every check has passed.
//!
//! There is deliberately no default handler, no fallback, no path, and no
//! dataset-specific knowledge: anything not exactly registered fails closed
//! with a typed error before content is opened.

mod activation;
mod authored_profile;
mod error;
mod handler;
mod limits;
mod payload;
mod planner;
mod prepared;
mod profile_discovery;
mod publication;
mod registry;
mod resource;

pub use activation::{
    ActivationError, NormalAuthorityDocuments, VerifiedNormalPublication, verify_normal_authority,
};
pub use authored_profile::{
    AuthoredCapabilityHandlerContract, AuthoredComponentHandlerContract, AuthoredComponentPayload,
    AuthoredRoleContract, AuthoredRootHandlerContract, AuthoredRootPayload, AuthoredRuntimeProfile,
    RUNTIME_PROFILE_CAPABILITY_URI, RUNTIME_PROFILE_SCHEMA, RUNTIME_PROFILE_VERSION,
    RuntimeProfileError, RuntimeProfileLimits, VerifiedResourcePayload,
};
pub use error::{RuntimePlanError, RuntimeRegistryError};
pub use handler::{
    BindingRequirement, CapabilityHandler, CapabilitySession, CapabilitySessionError,
    ComponentHandler, ComponentPlan, ComponentRejection, ContributionKind, ContributionRequirement,
    DocumentHandler, DocumentPlan, DocumentRejection, HandlerDescriptor, ResourceContribution,
};
pub use limits::RuntimePlanLimits;
pub use payload::{PreparedPayload, RuntimePayload};
pub use planner::RuntimePlanner;
pub use prepared::{PreparedContribution, PreparedSemanticWorld};
pub use profile_discovery::{
    DiscoveredRuntimeProfile, RuntimeProfileDiscoveryError, RuntimeProfileDiscoveryLimits,
    discover_runtime_profile,
};
pub use publication::{
    BoundResourceReadError, PublicationLimits, PublicationPaths, PublicationVerificationError,
    VerifiedPublication, verify_publication,
};
pub use registry::{HandlerKind, RuntimeRegistry, RuntimeRegistryBuilder};
pub use resource::{ContentAddress, ProviderError, ResourceProvider, STREAM_CHUNK_BYTES};
