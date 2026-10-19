//! Fail-closed negotiation and preparation of semantic world documents.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use worldrunner_world_model::{
    ResourceBinding, SemanticComponent, SemanticWorldDocument, validate_semantic_identifier,
    validate_structural_uri,
};

use crate::error::{RuntimePlanError, component_rejection};
use crate::handler::{
    CapabilityHandler, CapabilitySession, ComponentHandler, ComponentPlan, ContributionKind,
    ContributionRequirement, DocumentPlan, HandlerDescriptor, ResourceContribution,
};
use crate::limits::RuntimePlanLimits;
use crate::prepared::{PreparedContribution, PreparedSemanticWorld};
use crate::registry::RuntimeRegistry;
use crate::resource::{ResourceProvider, STREAM_CHUNK_BYTES, stream_binding};

/// Negotiates world documents against one caller-built registry.
///
/// The planner is stateless beyond its registry reference: preparation is a
/// pure function of the document, the caller's limits, and the provider.
pub struct RuntimePlanner<'registry> {
    registry: &'registry RuntimeRegistry,
}

struct NegotiatedComponent {
    component_id: String,
    plan: ComponentPlan,
}

struct StreamedResource {
    binding_id: String,
    contribution: ResourceContribution,
}

fn preflight_document(
    document: &SemanticWorldDocument,
    limits: &RuntimePlanLimits,
) -> Result<(), RuntimePlanError> {
    if document.components().len() > limits.max_components() {
        return Err(RuntimePlanError::ComponentLimitExceeded {
            max: limits.max_components(),
            observed: document.components().len(),
        });
    }
    if document.bindings().len() > limits.max_bindings() {
        return Err(RuntimePlanError::BindingLimitExceeded {
            max: limits.max_bindings(),
            observed: document.bindings().len(),
        });
    }
    let mut total = 0u64;
    for binding in document.bindings() {
        if binding.byte_length > limits.max_binding_bytes() {
            return Err(RuntimePlanError::BindingByteLimitExceeded {
                binding_id: binding.binding_id.clone(),
                max: limits.max_binding_bytes(),
                observed: binding.byte_length,
            });
        }
        total = total
            .checked_add(binding.byte_length)
            .ok_or(RuntimePlanError::CounterOverflow)?;
        if total > limits.max_total_binding_bytes() {
            return Err(RuntimePlanError::TotalBindingByteLimitExceeded {
                max: limits.max_total_binding_bytes(),
                observed: total,
            });
        }
    }
    Ok(())
}

impl<'registry> RuntimePlanner<'registry> {
    /// Creates a planner bound to `registry`.
    pub fn new(registry: &'registry RuntimeRegistry) -> Self {
        Self { registry }
    }

    /// Negotiates, verifies, and streams `document` into a prepared world.
    ///
    /// Every routing, claim, and content check must pass before the prepared
    /// world exists: the document is validated under `limits`, its schema must
    /// equal the registry's one exact schema, every component and binding must
    /// resolve a handler, every binding must be claimed by a requirement and
    /// match it exactly, each binding is then streamed exactly once while its
    /// content address is verified, and conflicting contributions or handler
    /// revisions abort preparation.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimePlanError`] for the first fail-closed refusal; no
    /// content is opened until every non-streaming check has passed.
    pub fn prepare(
        &self,
        document: &SemanticWorldDocument,
        limits: &RuntimePlanLimits,
        provider: &dyn ResourceProvider,
    ) -> Result<PreparedSemanticWorld, RuntimePlanError> {
        preflight_document(document, limits)?;
        document.validate(limits.semantic())?;
        if document.schema() != self.registry.document_schema() {
            return Err(RuntimePlanError::UnsupportedDocumentSchema {
                expected: self.registry.document_schema().clone(),
                observed: document.schema().clone(),
            });
        }
        let root_handler = self.registry.document_handler();
        let root = root_handler.plan(document).map_err(|rejection| {
            RuntimePlanError::DocumentRejected {
                handler_id: root_handler.descriptor().handler_id.clone(),
                rejection: Box::new(rejection),
            }
        })?;
        if &root.handler != root_handler.descriptor() {
            return Err(RuntimePlanError::HandlerIdentityMismatch {
                claimant: document.schema().tag.clone(),
                registered: root_handler.descriptor().clone(),
                reported: root.handler.clone(),
            });
        }
        validate_root_plan(&root)?;
        let component_handlers = self.resolve_component_handlers(document)?;
        let capability_handlers = self.resolve_capability_handlers(document)?;
        let components = Self::negotiate_components(document, &component_handlers)?;
        Self::verify_claims(document, &components)?;
        let mut buffer = vec![0u8; STREAM_CHUNK_BYTES].into_boxed_slice();
        let resources =
            Self::stream_all(document, provider, &capability_handlers, &mut buffer[..])?;
        Self::assemble(document, root, components, resources)
    }

    fn resolve_component_handlers(
        &self,
        document: &SemanticWorldDocument,
    ) -> Result<Vec<Arc<dyn ComponentHandler>>, RuntimePlanError> {
        document
            .components()
            .iter()
            .map(
                |component| match self.registry.component_handler(&component.schema_uri) {
                    Some(handler) => Ok(handler),
                    None => Err(RuntimePlanError::UnsupportedComponentSchema {
                        component_id: component.component_id.clone(),
                        schema_uri: component.schema_uri.clone(),
                    }),
                },
            )
            .collect()
    }

    fn resolve_capability_handlers(
        &self,
        document: &SemanticWorldDocument,
    ) -> Result<Vec<Arc<dyn CapabilityHandler>>, RuntimePlanError> {
        document
            .bindings()
            .iter()
            .map(
                |binding| match self.registry.capability_handler(&binding.capability_uri) {
                    Some(handler) => Ok(handler),
                    None => Err(RuntimePlanError::UnsupportedCapability {
                        binding_id: binding.binding_id.clone(),
                        capability_uri: binding.capability_uri.clone(),
                    }),
                },
            )
            .collect()
    }

    fn negotiate_components(
        document: &SemanticWorldDocument,
        handlers: &[Arc<dyn ComponentHandler>],
    ) -> Result<Vec<NegotiatedComponent>, RuntimePlanError> {
        let mut negotiated = Vec::with_capacity(document.components().len());
        for (component, handler) in document.components().iter().zip(handlers) {
            let plan = handler.plan(component).map_err(|rejection| {
                component_rejection(
                    &component.component_id,
                    &component.schema_uri,
                    handler.descriptor(),
                    rejection,
                )
            })?;
            if &plan.handler != handler.descriptor() {
                return Err(RuntimePlanError::HandlerIdentityMismatch {
                    claimant: component.component_id.clone(),
                    registered: handler.descriptor().clone(),
                    reported: plan.handler.clone(),
                });
            }
            validate_component_output(component, &plan)?;
            verify_requirement_order(component, &plan)?;
            negotiated.push(NegotiatedComponent {
                component_id: component.component_id.clone(),
                plan,
            });
        }
        Ok(negotiated)
    }

    fn verify_claims(
        document: &SemanticWorldDocument,
        components: &[NegotiatedComponent],
    ) -> Result<(), RuntimePlanError> {
        let bindings: BTreeMap<&str, &ResourceBinding> = document
            .bindings()
            .iter()
            .map(|binding| (binding.binding_id.as_str(), binding))
            .collect();
        let mut claimed: BTreeSet<String> = BTreeSet::new();
        for component in components {
            claim_requirements(&bindings, &mut claimed, component)?;
        }
        for binding in document.bindings() {
            if !claimed.contains(binding.binding_id.as_str()) {
                return Err(RuntimePlanError::UnconsumedBinding {
                    binding_id: binding.binding_id.clone(),
                    capability_uri: binding.capability_uri.clone(),
                });
            }
        }
        Ok(())
    }

    fn stream_all(
        document: &SemanticWorldDocument,
        provider: &dyn ResourceProvider,
        capability_handlers: &[Arc<dyn CapabilityHandler>],
        buffer: &mut [u8],
    ) -> Result<Vec<StreamedResource>, RuntimePlanError> {
        let sessions = start_all_sessions(document, capability_handlers)?;
        let mut streamed = Vec::with_capacity(document.bindings().len());
        for ((binding, handler), session) in document
            .bindings()
            .iter()
            .zip(capability_handlers)
            .zip(sessions)
        {
            let contribution =
                stream_binding(binding, provider, session, handler.descriptor(), buffer)?;
            validate_resource_contribution(binding, handler.descriptor(), &contribution)?;
            streamed.push(StreamedResource {
                binding_id: binding.binding_id.clone(),
                contribution,
            });
        }
        Ok(streamed)
    }

    fn assemble(
        document: &SemanticWorldDocument,
        root: DocumentPlan,
        components: Vec<NegotiatedComponent>,
        resources: Vec<StreamedResource>,
    ) -> Result<PreparedSemanticWorld, RuntimePlanError> {
        verify_root_closure(&root, &components, &resources)?;
        let mut revisions: BTreeMap<String, u64> = BTreeMap::new();
        record_revision(&mut revisions, &root.handler, &document.schema().tag)?;
        let mut prepared_components = Vec::with_capacity(components.len());
        for component in components {
            record_revision(
                &mut revisions,
                &component.plan.handler,
                &component.component_id,
            )?;
            prepared_components.push(PreparedContribution::new(
                component.component_id,
                component.plan.handler,
                component.plan.payload_contract_uri,
                component.plan.payload,
            ));
        }
        let mut prepared_resources = Vec::with_capacity(resources.len());
        for resource in resources {
            record_revision(
                &mut revisions,
                &resource.contribution.handler,
                &resource.binding_id,
            )?;
            prepared_resources.push(PreparedContribution::new(
                resource.binding_id,
                resource.contribution.handler,
                resource.contribution.payload_contract_uri,
                resource.contribution.payload,
            ));
        }
        prepared_components.sort_by(|left, right| {
            (&left.payload_contract_uri, &left.claimant)
                .cmp(&(&right.payload_contract_uri, &right.claimant))
        });
        prepared_resources.sort_by(|left, right| {
            (&left.payload_contract_uri, &left.claimant)
                .cmp(&(&right.payload_contract_uri, &right.claimant))
        });
        Ok(PreparedSemanticWorld::new(
            document.schema().clone(),
            *document.source(),
            root,
            prepared_components,
            prepared_resources,
            revisions,
        ))
    }
}

fn start_all_sessions(
    document: &SemanticWorldDocument,
    handlers: &[Arc<dyn CapabilityHandler>],
) -> Result<Vec<Box<dyn CapabilitySession>>, RuntimePlanError> {
    document
        .bindings()
        .iter()
        .zip(handlers)
        .map(|(binding, handler)| {
            handler.start_session(binding).map_err(|error| {
                RuntimePlanError::from_session(binding, handler.descriptor(), error)
            })
        })
        .collect()
}

fn validate_resource_contribution(
    binding: &ResourceBinding,
    registered: &HandlerDescriptor,
    contribution: &ResourceContribution,
) -> Result<(), RuntimePlanError> {
    if &contribution.handler != registered {
        return Err(RuntimePlanError::HandlerIdentityMismatch {
            claimant: binding.binding_id.clone(),
            registered: registered.clone(),
            reported: contribution.handler.clone(),
        });
    }
    validate_output_uri(
        &binding.binding_id,
        "payload_contract_uri",
        &contribution.payload_contract_uri,
    )
}

fn validate_root_plan(plan: &DocumentPlan) -> Result<(), RuntimePlanError> {
    for requirement in &plan.contribution_requirements {
        validate_output_identifier(&plan.handler.handler_id, "claimant", &requirement.claimant)?;
        validate_output_uri(
            &requirement.claimant,
            "payload_contract_uri",
            &requirement.payload_contract_uri,
        )?;
    }
    for pair in plan.contribution_requirements.windows(2) {
        match pair[0].cmp(&pair[1]) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(RuntimePlanError::DuplicateContributionRequirement {
                    requirement: Box::new(pair[1].clone()),
                });
            }
            Ordering::Greater => {
                return Err(RuntimePlanError::InvalidContributionRequirementOrder {
                    previous: Box::new(pair[0].clone()),
                    next: Box::new(pair[1].clone()),
                });
            }
        }
    }
    Ok(())
}

fn verify_root_closure(
    root: &DocumentPlan,
    components: &[NegotiatedComponent],
    resources: &[StreamedResource],
) -> Result<(), RuntimePlanError> {
    let mut actual = BTreeSet::new();
    actual.extend(components.iter().map(|component| {
        ContributionRequirement::new(
            ContributionKind::Component,
            &component.component_id,
            &component.plan.payload_contract_uri,
        )
    }));
    actual.extend(resources.iter().map(|resource| {
        ContributionRequirement::new(
            ContributionKind::Resource,
            &resource.binding_id,
            &resource.contribution.payload_contract_uri,
        )
    }));
    for requirement in &root.contribution_requirements {
        if !actual.remove(requirement) {
            return Err(RuntimePlanError::MissingContribution {
                requirement: Box::new(requirement.clone()),
            });
        }
    }
    if let Some(contribution) = actual.into_iter().next() {
        return Err(RuntimePlanError::UnexpectedContribution {
            contribution: Box::new(contribution),
        });
    }
    Ok(())
}

fn validate_component_output(
    component: &SemanticComponent,
    plan: &ComponentPlan,
) -> Result<(), RuntimePlanError> {
    validate_output_uri(
        &component.component_id,
        "payload_contract_uri",
        &plan.payload_contract_uri,
    )?;
    for requirement in &plan.binding_requirements {
        validate_output_identifier(
            &component.component_id,
            "binding_requirement.binding_id",
            &requirement.binding_id,
        )?;
        validate_output_uri(
            &component.component_id,
            "binding_requirement.capability_uri",
            &requirement.capability_uri,
        )?;
        if let Some(element_uri) = &requirement.element_uri {
            validate_output_uri(
                &component.component_id,
                "binding_requirement.element_uri",
                element_uri,
            )?;
        }
    }
    Ok(())
}

fn validate_output_identifier(
    claimant: &str,
    field: &'static str,
    value: &str,
) -> Result<(), RuntimePlanError> {
    validate_semantic_identifier(value).map_err(|source| RuntimePlanError::InvalidHandlerOutput {
        claimant: claimant.to_owned(),
        field,
        value: value.to_owned(),
        source,
    })
}

fn validate_output_uri(
    claimant: &str,
    field: &'static str,
    value: &str,
) -> Result<(), RuntimePlanError> {
    validate_structural_uri(value).map_err(|source| RuntimePlanError::InvalidHandlerOutput {
        claimant: claimant.to_owned(),
        field,
        value: value.to_owned(),
        source,
    })
}

fn verify_requirement_order(
    component: &SemanticComponent,
    plan: &ComponentPlan,
) -> Result<(), RuntimePlanError> {
    for pair in plan.binding_requirements.windows(2) {
        match pair[0].cmp(&pair[1]) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(RuntimePlanError::DuplicateRequirement {
                    component_id: component.component_id.clone(),
                    requirement: Box::new(pair[1].clone()),
                });
            }
            Ordering::Greater => {
                return Err(RuntimePlanError::InvalidRequirementOrder {
                    component_id: component.component_id.clone(),
                    previous: Box::new(pair[0].clone()),
                    next: Box::new(pair[1].clone()),
                });
            }
        }
    }
    Ok(())
}

fn claim_requirements(
    bindings: &BTreeMap<&str, &ResourceBinding>,
    claimed: &mut BTreeSet<String>,
    component: &NegotiatedComponent,
) -> Result<(), RuntimePlanError> {
    for requirement in &component.plan.binding_requirements {
        let binding = bindings
            .get(requirement.binding_id.as_str())
            .ok_or_else(|| RuntimePlanError::UnresolvedBindingRequirement {
                component_id: component.component_id.clone(),
                binding_id: requirement.binding_id.clone(),
            })?;
        if binding.capability_uri != requirement.capability_uri {
            return Err(RuntimePlanError::BindingCapabilityMismatch {
                component_id: component.component_id.clone(),
                binding_id: requirement.binding_id.clone(),
                required: requirement.capability_uri.clone(),
                observed: binding.capability_uri.clone(),
            });
        }
        if binding.element_uri != requirement.element_uri {
            return Err(RuntimePlanError::BindingElementMismatch {
                component_id: component.component_id.clone(),
                binding_id: requirement.binding_id.clone(),
                required: requirement.element_uri.clone(),
                observed: binding.element_uri.clone(),
            });
        }
        claimed.insert(binding.binding_id.clone());
    }
    Ok(())
}

fn record_revision(
    revisions: &mut BTreeMap<String, u64>,
    descriptor: &HandlerDescriptor,
    claimant: &str,
) -> Result<(), RuntimePlanError> {
    if let Some(existing) = revisions.get(&descriptor.handler_id) {
        if *existing != descriptor.revision {
            return Err(RuntimePlanError::HandlerIdentityMismatch {
                claimant: claimant.to_owned(),
                registered: HandlerDescriptor::new(&descriptor.handler_id, *existing),
                reported: descriptor.clone(),
            });
        }
        return Ok(());
    }
    revisions.insert(descriptor.handler_id.clone(), descriptor.revision);
    Ok(())
}
