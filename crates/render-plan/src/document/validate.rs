//! Shared fail-closed validation helpers for authored render-contract documents.
//!
//! Every helper reports through [`ContractDocumentError`] with an explicit
//! field name so authored documents are rejected with precise context. All
//! derived quantities are computed through checked arithmetic only.

use std::cmp::Ordering;

use worldrunner_world_model::{validate_semantic_identifier, validate_structural_uri};

use crate::document::error::ContractDocumentError;

/// Validates one identifier field against the universal identifier rules.
pub(crate) fn require_identifier(
    field: &'static str,
    identifier: &str,
) -> Result<(), ContractDocumentError> {
    validate_semantic_identifier(identifier)
        .map_err(|source| ContractDocumentError::InvalidIdentifier { field, source })
}

/// Validates one URI field against the universal structural URI rules.
pub(crate) fn require_uri(field: &'static str, uri: &str) -> Result<(), ContractDocumentError> {
    validate_structural_uri(uri)
        .map_err(|source| ContractDocumentError::InvalidUri { field, source })
}

/// Requires keyed entries in strictly ascending, duplicate-free key order.
pub(crate) fn require_sorted_unique<K>(
    field: &'static str,
    keys: impl Iterator<Item = K>,
) -> Result<(), ContractDocumentError>
where
    K: Ord + std::fmt::Debug,
{
    let mut previous: Option<K> = None;
    for key in keys {
        if let Some(prev) = previous {
            match prev.cmp(&key) {
                Ordering::Less => {}
                Ordering::Equal => {
                    return Err(ContractDocumentError::DuplicateKey {
                        field,
                        key: format!("{key:?}"),
                    });
                }
                Ordering::Greater => {
                    return Err(ContractDocumentError::UnsortedEntries { field });
                }
            }
        }
        previous = Some(key);
    }
    Ok(())
}

/// Requires keyed entries free of duplicates, in any order.
pub(crate) fn require_unique<K>(
    field: &'static str,
    keys: impl Iterator<Item = K>,
) -> Result<(), ContractDocumentError>
where
    K: Ord + std::fmt::Debug,
{
    let mut seen: Vec<K> = Vec::new();
    for key in keys {
        match seen.binary_search(&key) {
            Ok(_) => {
                return Err(ContractDocumentError::DuplicateKey {
                    field,
                    key: format!("{key:?}"),
                });
            }
            Err(index) => seen.insert(index, key),
        }
    }
    Ok(())
}

/// Requires one signed value inside inclusive bounds.
pub(crate) fn require_range(
    field: &'static str,
    min: i128,
    max: i128,
    observed: i128,
) -> Result<(), ContractDocumentError> {
    if observed < min || observed > max {
        return Err(ContractDocumentError::OutOfRange {
            field,
            min,
            max,
            observed,
        });
    }
    Ok(())
}

/// Requires one strictly positive `u32`.
pub(crate) fn require_positive_u32(
    field: &'static str,
    observed: u32,
) -> Result<(), ContractDocumentError> {
    require_range(field, 1, i128::from(u32::MAX), i128::from(observed))
}

/// Requires one strictly positive `u64`.
pub(crate) fn require_positive_u64(
    field: &'static str,
    observed: u64,
) -> Result<(), ContractDocumentError> {
    require_range(field, 1, i128::from(u64::MAX), i128::from(observed))
}

/// Checked unsigned multiplication.
pub(crate) fn checked_mul_u64(
    field: &'static str,
    left: u64,
    right: u64,
) -> Result<u64, ContractDocumentError> {
    left.checked_mul(right)
        .ok_or(ContractDocumentError::ArithmeticOverflow { field })
}

/// Checked signed addition.
pub(crate) fn checked_add_i64(
    field: &'static str,
    left: i64,
    right: i64,
) -> Result<i64, ContractDocumentError> {
    left.checked_add(right)
        .ok_or(ContractDocumentError::ArithmeticOverflow { field })
}

/// Checked signed subtraction.
pub(crate) fn checked_sub_i64(
    field: &'static str,
    left: i64,
    right: i64,
) -> Result<i64, ContractDocumentError> {
    left.checked_sub(right)
        .ok_or(ContractDocumentError::ArithmeticOverflow { field })
}
