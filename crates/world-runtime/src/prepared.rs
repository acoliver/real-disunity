//! Read-only result of one fully verified negotiation.

use std::collections::BTreeMap;

use worldrunner_world_model::{SchemaDescriptor, SourceIdentity};

use crate::handler::{DocumentPlan, HandlerDescriptor};
use crate::payload::PreparedPayload;

/// One prepared contribution, from a component plan or a resource session.
#[derive(Clone, Debug)]
pub struct PreparedContribution {
    /// Identifier of the component or binding that produced this contribution.
    pub claimant: String,
    /// Descriptor of the handler that produced this contribution.
    pub handler: HandlerDescriptor,
    /// URI naming the contribution inside the prepared world.
    pub payload_contract_uri: String,
    /// Immutable renderer-ready payload for the exact contribution contract.
    pub payload: PreparedPayload,
}

impl PreparedContribution {
    /// Adopts the contribution parts.
    pub fn new(
        claimant: String,
        handler: HandlerDescriptor,
        payload_contract_uri: String,
        payload: PreparedPayload,
    ) -> Self {
        Self {
            claimant,
            handler,
            payload_contract_uri,
            payload,
        }
    }
}

/// Opaque, read-only world produced after every negotiation check passed.
///
/// Instances exist only as the output of [`crate::RuntimePlanner::prepare`];
/// contributions are sorted ascending by payload contract URI and claimant and the handler
/// revision map records every handler that actually negotiated.
#[derive(Debug)]
pub struct PreparedSemanticWorld {
    schema: SchemaDescriptor,
    source: SourceIdentity,
    root_handler: HandlerDescriptor,
    root_payload: PreparedPayload,
    components: Vec<PreparedContribution>,
    resources: Vec<PreparedContribution>,
    handler_revisions: BTreeMap<String, u64>,
}

impl PreparedSemanticWorld {
    /// Adopts the fully verified preparation parts.
    pub(crate) fn new(
        schema: SchemaDescriptor,
        source: SourceIdentity,
        root: DocumentPlan,
        components: Vec<PreparedContribution>,
        resources: Vec<PreparedContribution>,
        handler_revisions: BTreeMap<String, u64>,
    ) -> Self {
        Self {
            schema,
            source,
            root_handler: root.handler,
            root_payload: root.payload,
            components,
            resources,
            handler_revisions,
        }
    }

    /// Returns the exact schema the world was negotiated under.
    pub fn schema(&self) -> &SchemaDescriptor {
        &self.schema
    }

    /// Returns the exact identity of the source the world was derived from.
    pub fn source(&self) -> &SourceIdentity {
        &self.source
    }

    /// Returns the descriptor of the handler that proved root closure.
    pub fn root_handler(&self) -> &HandlerDescriptor {
        &self.root_handler
    }

    /// Returns the immutable typed document-root payload.
    pub fn root_payload(&self) -> &PreparedPayload {
        &self.root_payload
    }

    /// Returns the prepared component contributions, sorted by payload contract URI and claimant.
    pub fn component_contributions(&self) -> &[PreparedContribution] {
        &self.components
    }

    /// Returns the prepared resource contributions, sorted by payload contract URI and claimant.
    pub fn resource_contributions(&self) -> &[PreparedContribution] {
        &self.resources
    }

    /// Returns the deterministic revision of every handler that negotiated.
    pub fn handler_revisions(&self) -> &BTreeMap<String, u64> {
        &self.handler_revisions
    }
}
