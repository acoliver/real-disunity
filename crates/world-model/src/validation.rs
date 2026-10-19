//! Explicit, caller-bounded validation for semantic structures.

use std::cmp::Ordering;

use thiserror::Error;

use crate::digest::DigestError;
use crate::document::ResourceBinding;
use crate::identity::SemanticComponent;
use crate::value::{SemanticMap, SemanticValue};

/// Operational bounds for semantic validation, supplied explicitly by callers.
///
/// There is deliberately no default: embedding dataset-specific population
/// counts would smuggle corpus assumptions into a universal contract. Callers
/// choose limits that match their operational envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationLimits {
    /// Maximum nesting depth of a payload, counting the root node as depth 1.
    pub max_depth: usize,
    /// Maximum number of value nodes in a single component payload.
    pub max_nodes: usize,
    /// Maximum total UTF-8 byte length of text scalars in a single payload.
    pub max_text_bytes: usize,
}

/// Everything that can go wrong while building or validating the model.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorldModelError {
    /// A payload nests deeper than [`ValidationLimits::max_depth`].
    #[error("semantic value nesting exceeds max_depth {max_depth}")]
    DepthLimitExceeded {
        /// The limit that was exceeded.
        max_depth: usize,
    },
    /// A payload contains more value nodes than [`ValidationLimits::max_nodes`].
    #[error("semantic value node count exceeds max_nodes {max_nodes}")]
    NodeLimitExceeded {
        /// The limit that was exceeded.
        max_nodes: usize,
    },
    /// A payload contains more text bytes than [`ValidationLimits::max_text_bytes`].
    #[error("semantic value text bytes exceed max_text_bytes {max_text_bytes}")]
    TextByteLimitExceeded {
        /// The limit that was exceeded.
        max_text_bytes: usize,
    },
    /// A checked counter overflowed while measuring a structure.
    #[error("semantic structure exceeds hashing counter range")]
    CounterOverflow,
    /// A map key was empty.
    #[error("map keys must be nonempty")]
    EmptyMapKey,
    /// A map key contained a control character.
    #[error("map keys must not contain control characters")]
    ControlCharacterInMapKey,
    /// An identifier was empty or contained control characters.
    #[error("identifier must be nonempty and free of control characters: {identifier:?}")]
    InvalidIdentifier {
        /// The rejected identifier.
        identifier: String,
    },
    /// A URI lacked structural form (scheme, separator, clean characters).
    #[error("URI needs a nonempty scheme, a ':' separator, no whitespace or control: {uri:?}")]
    InvalidUri {
        /// The rejected URI.
        uri: String,
    },
    /// The declared source identity does not match the canonical components.
    #[error("declared source identity {declared} does not match derived identity {derived}")]
    SourceIdentityMismatch {
        /// Identity carried by the document.
        declared: String,
        /// Identity derived from canonical component semantics.
        derived: String,
    },
    /// Components were not supplied in strictly ascending identifier order.
    #[error("components must be provided in strictly ascending component_id order")]
    UnsortedComponents,
    /// Two components share one identifier.
    #[error("duplicate component_id: {identifier:?}")]
    DuplicateComponent {
        /// The duplicated identifier.
        identifier: String,
    },
    /// Bindings were not supplied in strictly ascending identifier order.
    #[error("bindings must be provided in strictly ascending binding_id order")]
    UnsortedBindings,
    /// Two bindings share one identifier.
    #[error("duplicate binding_id: {identifier:?}")]
    DuplicateBinding {
        /// The duplicated identifier.
        identifier: String,
    },
    /// A `BindingRef` did not resolve to any binding in the document.
    #[error("binding reference does not resolve to any binding: {identifier:?}")]
    UnresolvedBindingRef {
        /// The unresolved reference.
        identifier: String,
    },
    /// A digest was malformed or reserved.
    #[error(transparent)]
    Digest(#[from] DigestError),
}

#[derive(Debug, Default)]
struct ValidationCounters {
    nodes: usize,
    text_bytes: usize,
}

/// Validates one corpus-free decoded source and its pathless source resources.
pub(crate) fn validate_decoded_save(
    components: &[SemanticComponent],
    resources: &[ResourceBinding],
    limits: &ValidationLimits,
) -> Result<(), WorldModelError> {
    validate_components(components, limits)?;
    for resource in resources {
        validate_binding(resource)?;
    }
    Ok(())
}

/// Validates limits-independent decoded-source structure.
pub(crate) fn validate_decoded_save_structure(
    components: &[SemanticComponent],
    resources: &[ResourceBinding],
) -> Result<(), WorldModelError> {
    validate_component_order(components)?;
    validate_binding_order(resources)?;
    for component in components {
        validate_component_structure(component)?;
    }
    for resource in resources {
        validate_binding(resource)?;
    }
    Ok(())
}

/// Validates component payloads against one aggregate caller-supplied budget.
pub(crate) fn validate_components(
    components: &[SemanticComponent],
    limits: &ValidationLimits,
) -> Result<(), WorldModelError> {
    let mut counters = ValidationCounters::default();
    for component in components {
        validate_component_structure(component)?;
        validate_map(&component.payload, limits, 1, &mut counters)?;
    }
    Ok(())
}

/// Validates the limits-independent parts of one component.
///
/// Covers the component identifier, schema URI, map key shape, and
/// `BindingRef` identifiers; payload budgets remain caller-supplied.
pub(crate) fn validate_component_structure(
    component: &SemanticComponent,
) -> Result<(), WorldModelError> {
    validate_identifier(&component.component_id)?;
    validate_uri(&component.schema_uri)?;
    validate_map_structure(&component.payload)
}

/// Validates identifiers and URIs of one resource binding.
pub(crate) fn validate_binding(binding: &ResourceBinding) -> Result<(), WorldModelError> {
    validate_identifier(&binding.binding_id)?;
    validate_uri(&binding.capability_uri)?;
    match &binding.element_uri {
        Some(uri) => validate_uri(uri),
        None => Ok(()),
    }
}

/// Requires every `BindingRef` in the payloads to resolve to a binding.
pub(crate) fn validate_binding_resolution(
    components: &[SemanticComponent],
    bindings: &[ResourceBinding],
) -> Result<(), WorldModelError> {
    let mut references = Vec::new();
    for component in components {
        component
            .payload
            .visit_binding_refs(&mut |binding_id| references.push(binding_id.to_owned()));
    }
    for reference in references {
        let resolves = bindings
            .iter()
            .any(|binding| binding.binding_id == reference);
        if !resolves {
            return Err(WorldModelError::UnresolvedBindingRef {
                identifier: reference,
            });
        }
    }
    Ok(())
}

/// Requires components in strictly ascending, duplicate-free identifier order.
pub(crate) fn validate_component_order(
    components: &[SemanticComponent],
) -> Result<(), WorldModelError> {
    for window in components.windows(2) {
        let previous = &window[0];
        let next = &window[1];
        match previous.component_id.cmp(&next.component_id) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(WorldModelError::DuplicateComponent {
                    identifier: next.component_id.clone(),
                });
            }
            Ordering::Greater => return Err(WorldModelError::UnsortedComponents),
        }
    }
    Ok(())
}

/// Requires bindings in strictly ascending, duplicate-free identifier order.
pub(crate) fn validate_binding_order(bindings: &[ResourceBinding]) -> Result<(), WorldModelError> {
    for window in bindings.windows(2) {
        let previous = &window[0];
        let next = &window[1];
        match previous.binding_id.cmp(&next.binding_id) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(WorldModelError::DuplicateBinding {
                    identifier: next.binding_id.clone(),
                });
            }
            Ordering::Greater => return Err(WorldModelError::UnsortedBindings),
        }
    }
    Ok(())
}

/// Validates limits-independent world-document structure.
pub(crate) fn validate_structure(
    schema_uri: &str,
    components: &[SemanticComponent],
    bindings: &[ResourceBinding],
) -> Result<(), WorldModelError> {
    validate_uri(schema_uri)?;
    validate_component_order(components)?;
    validate_binding_order(bindings)?;
    for component in components {
        validate_component_structure(component)?;
    }
    for binding in bindings {
        validate_binding(binding)?;
    }
    validate_binding_resolution(components, bindings)
}

fn validate_map(
    map: &SemanticMap,
    limits: &ValidationLimits,
    depth: usize,
    counters: &mut ValidationCounters,
) -> Result<(), WorldModelError> {
    if depth > limits.max_depth {
        return Err(WorldModelError::DepthLimitExceeded {
            max_depth: limits.max_depth,
        });
    }
    bump_nodes(counters, limits)?;
    validate_map_entries(map, limits, depth, counters)
}

fn validate_map_entries(
    map: &SemanticMap,
    limits: &ValidationLimits,
    depth: usize,
    counters: &mut ValidationCounters,
) -> Result<(), WorldModelError> {
    for (key, value) in map.iter() {
        validate_map_key(key)?;
        validate_value(value, limits, depth + 1, counters)?;
    }
    Ok(())
}

fn validate_value(
    value: &SemanticValue,
    limits: &ValidationLimits,
    depth: usize,
    counters: &mut ValidationCounters,
) -> Result<(), WorldModelError> {
    if depth > limits.max_depth {
        return Err(WorldModelError::DepthLimitExceeded {
            max_depth: limits.max_depth,
        });
    }
    bump_nodes(counters, limits)?;
    match value {
        SemanticValue::Null
        | SemanticValue::Bool(_)
        | SemanticValue::Signed(_)
        | SemanticValue::Unsigned(_) => Ok(()),
        SemanticValue::Text(text) => bump_text_bytes(counters, limits, text.len()),
        SemanticValue::Sequence(items) => {
            for item in items {
                validate_value(item, limits, depth + 1, counters)?;
            }
            Ok(())
        }
        SemanticValue::Map(map) => validate_map_entries(map, limits, depth, counters),
        SemanticValue::BindingRef(binding_id) => validate_identifier(binding_id),
    }
}

fn bump_nodes(
    counters: &mut ValidationCounters,
    limits: &ValidationLimits,
) -> Result<(), WorldModelError> {
    counters.nodes = counters
        .nodes
        .checked_add(1)
        .ok_or(WorldModelError::CounterOverflow)?;
    if counters.nodes > limits.max_nodes {
        return Err(WorldModelError::NodeLimitExceeded {
            max_nodes: limits.max_nodes,
        });
    }
    Ok(())
}

fn bump_text_bytes(
    counters: &mut ValidationCounters,
    limits: &ValidationLimits,
    length: usize,
) -> Result<(), WorldModelError> {
    counters.text_bytes = counters
        .text_bytes
        .checked_add(length)
        .ok_or(WorldModelError::CounterOverflow)?;
    if counters.text_bytes > limits.max_text_bytes {
        return Err(WorldModelError::TextByteLimitExceeded {
            max_text_bytes: limits.max_text_bytes,
        });
    }
    Ok(())
}

fn validate_map_key(key: &str) -> Result<(), WorldModelError> {
    if key.is_empty() {
        return Err(WorldModelError::EmptyMapKey);
    }
    if has_control(key) {
        return Err(WorldModelError::ControlCharacterInMapKey);
    }
    Ok(())
}

/// Validates one universal semantic identifier.
///
/// Identifiers must be nonempty and contain no control characters.
pub fn validate_semantic_identifier(identifier: &str) -> Result<(), WorldModelError> {
    validate_identifier(identifier)
}

/// Validates one structural URI used for schema and capability routing.
pub fn validate_structural_uri(uri: &str) -> Result<(), WorldModelError> {
    validate_uri(uri)
}

pub(crate) fn validate_identifier(identifier: &str) -> Result<(), WorldModelError> {
    if identifier.is_empty() || has_control(identifier) {
        return Err(WorldModelError::InvalidIdentifier {
            identifier: identifier.to_owned(),
        });
    }
    Ok(())
}

pub(crate) fn validate_uri(uri: &str) -> Result<(), WorldModelError> {
    let valid = has_separator_with_valid_scheme(uri) && !has_control_or_whitespace(uri);
    if valid {
        Ok(())
    } else {
        Err(WorldModelError::InvalidUri {
            uri: uri.to_owned(),
        })
    }
}

fn validate_map_structure(map: &SemanticMap) -> Result<(), WorldModelError> {
    for (key, value) in map.iter() {
        validate_map_key(key)?;
        validate_value_structure(value)?;
    }
    Ok(())
}

fn validate_value_structure(value: &SemanticValue) -> Result<(), WorldModelError> {
    match value {
        SemanticValue::Sequence(items) => {
            for item in items {
                validate_value_structure(item)?;
            }
            Ok(())
        }
        SemanticValue::Map(map) => validate_map_structure(map),
        SemanticValue::BindingRef(binding_id) => validate_identifier(binding_id),
        _ => Ok(()),
    }
}

fn has_separator_with_valid_scheme(uri: &str) -> bool {
    match uri.split_once(':') {
        Some((scheme, _)) => is_valid_scheme(scheme),
        None => false,
    }
}

fn is_valid_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn has_control(text: &str) -> bool {
    text.chars().any(char::is_control)
}

fn has_control_or_whitespace(text: &str) -> bool {
    text.chars().any(|c| c.is_control() || c.is_whitespace())
}
