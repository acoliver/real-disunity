//! Versioned, deterministic contracts for importing original world saves.
//!
//! This crate owns build-pipeline governance documents rather than runtime world data. It is a
//! leaf schema crate: import tools and runtime validation may depend on it, but it does not depend
//! on either pipeline.
//!
//! The document boundary is: a generic [`SaveBundleManifest`] enumerating arbitrary observed
//! artifacts, a [`SourceObservationLock`] pinning only observed save-bundle identity, an
//! independently authored [`CorpusManifest`], a [`CorpusBindingDocument`] and a
//! [`CapabilityGraphDocument`].

mod capability_graph;
mod corpus_binding;
mod corpus_manifest;
mod save_bundle;
mod source_lock;
mod validation;
mod version;

pub use capability_graph::{
    CapabilityGraphDocument, CapabilityGraphError, RoleBindingAssignment, RuntimeOutputRecipe,
    SemanticRecipe, SourceSelector,
};
pub use corpus_binding::{CorpusBindingDocument, CorpusBindingError, CorpusBindingRecord};
pub use corpus_manifest::{
    CorpusArtifact, CorpusCapabilityDocument, CorpusManifest, CorpusManifestError,
};
pub use save_bundle::{
    ArtifactDescriptor, AxisDescriptor, ComponentSetDescriptor, SampledFieldDescriptor,
    SaveBundleEntry, SaveBundleError, SaveBundleManifest,
};
pub use source_lock::{ObservedArtifactEntry, SourceLockError, SourceObservationLock};
pub use version::{
    CAPABILITY_GRAPH_DOCUMENT_URI, CAPABILITY_GRAPH_SCHEMA, CAPABILITY_GRAPH_VERSION,
    CORPUS_BINDING_DOCUMENT_URI, CORPUS_BINDING_SCHEMA, CORPUS_BINDING_VERSION,
    CORPUS_MANIFEST_SCHEMA, CORPUS_MANIFEST_VERSION, DocumentHeader, SAVE_BUNDLE_MANIFEST_SCHEMA,
    SAVE_BUNDLE_MANIFEST_VERSION, SOURCE_OBSERVATION_LOCK_SCHEMA, SOURCE_OBSERVATION_LOCK_VERSION,
    VersionedDocumentError,
};
