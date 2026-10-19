//! Versioned, deterministic contracts for importing original world saves.
//!
//! This crate owns build-pipeline governance documents rather than runtime world data. It is a
//! leaf schema crate: import tools and runtime validation may depend on it, but it does not depend
//! on either pipeline.
//!
//! The document boundary is: a generic [`SaveBundleManifest`] enumerating arbitrary observed
//! artifacts, a [`SourceObservationLock`] pinning only observed save-bundle identity, an
//! independently authored [`CorpusManifest`], governance [`ValidationAttestation`]s, an
//! [`ImportReceipt`] binding one import run together, and the shared
//! [`EnvironmentScopePolicy`]. Census data exists only as receipt-side stream summaries.
//! It also owns the source-neutral [`PairedCaptureManifest`] that pairs one observed source
//! capture with one replayed capture for comparability-graded differential measurement.

mod attestation;
mod capability_graph;
mod census;
mod corpus_binding;
mod corpus_manifest;
mod paired_capture;
mod publication_set;
mod receipt;
mod save_bundle;
mod scope_policy;
mod source_lock;
mod validation;
mod version;

pub use attestation::{
    AttestationEvidenceArtifact, AttestationOutcome, ValidationAttestation,
    ValidationAttestationError,
};
pub use capability_graph::{
    CapabilityGraphDocument, CapabilityGraphError, RoleBindingAssignment, RuntimeOutputRecipe,
    SemanticRecipe, SourceSelector,
};
pub use census::{
    BindingClosureSummary, CensusBucket, CensusFunnel, CensusResolution, CensusSummary,
    CensusSummaryError, OptionalTextPartition,
};
pub use corpus_binding::{CorpusBindingDocument, CorpusBindingError, CorpusBindingRecord};
pub use corpus_manifest::{
    CorpusArtifact, CorpusCapabilityDocument, CorpusManifest, CorpusManifestError,
};
pub use paired_capture::{
    BuildIdentity, CORE_STATE_KEYS, CameraPose, CaptureSide, CaptureState, ComparabilityClaim,
    ComparabilityGrade, ObservationRole, Observed, OutputArtifact, PairedCaptureError,
    PairedCaptureManifest, PoseRecord, PoseTolerance, ProjectionRecord, ViewportRecord,
};
pub use publication_set::{PublicationSetDescriptor, PublishedArtifact};
pub use receipt::{
    ImportReceipt, ImportReceiptError, ReceiptOutputArtifact, ReceiptPromotionError,
    require_promotion_equivalence,
};
pub use save_bundle::{
    ArtifactDescriptor, AxisDescriptor, ComponentSetDescriptor, SampledFieldDescriptor,
    SaveBundleEntry, SaveBundleError, SaveBundleManifest,
};
pub use scope_policy::{
    EnvironmentScopePolicy, MatchKind, ScopeDisposition, ScopePolicyError, ScopeResolutionError,
    ScopeRule, ScopeSelector, validate_class_ancestry,
};
pub use source_lock::{ObservedArtifactEntry, SourceLockError, SourceObservationLock};
pub use version::{
    CAPABILITY_GRAPH_DOCUMENT_URI, CAPABILITY_GRAPH_SCHEMA, CAPABILITY_GRAPH_VERSION,
    CORPUS_BINDING_DOCUMENT_URI, CORPUS_BINDING_SCHEMA, CORPUS_BINDING_VERSION,
    CORPUS_MANIFEST_SCHEMA, CORPUS_MANIFEST_VERSION, DocumentHeader,
    ENVIRONMENT_SCOPE_POLICY_SCHEMA, IMPORT_RECEIPT_SCHEMA, IMPORT_RECEIPT_VERSION,
    PAIRED_CAPTURE_MANIFEST_SCHEMA, PAIRED_CAPTURE_MANIFEST_VERSION, PUBLICATION_SET_SCHEMA,
    PUBLICATION_SET_VERSION, SAVE_BUNDLE_MANIFEST_SCHEMA, SAVE_BUNDLE_MANIFEST_VERSION,
    SOURCE_OBSERVATION_LOCK_SCHEMA, SOURCE_OBSERVATION_LOCK_VERSION, VALIDATION_ATTESTATION_SCHEMA,
    VALIDATION_ATTESTATION_VERSION, VersionedDocumentError,
};
