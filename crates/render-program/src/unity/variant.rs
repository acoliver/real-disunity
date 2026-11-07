//! Choosing a program's compiled variant from the enabled keywords.

use std::collections::BTreeSet;

use worldrunner_unity_serialized::shader_program::{PlayerSubProgram, Program, SubProgram};

use crate::variant::{self as neutral, FeatureId, FeatureSet, UnknownFeatures};

/// No variant matches the enabled keywords.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VariantError {
    #[error("no variant of {available} matches the keyword set {wanted:?}")]
    NoMatch { wanted: Vec<u16>, available: usize },
}

/// The variant whose keyword set equals the enabled keywords the stage declares. A keyword the
/// stage does not declare is ignored.
///
/// # Errors
/// Fails when no variant has exactly that keyword set.
pub fn select_variant<'a, V>(
    variants: &'a [V],
    keywords: impl Fn(&V) -> &[u16],
    declared: &[u16],
    enabled: &BTreeSet<u16>,
) -> Result<&'a V, VariantError> {
    let features =
        |indices: &[u16]| -> FeatureSet { indices.iter().map(|index| feature(*index)).collect() };
    let enabled_features: FeatureSet = enabled.iter().map(|index| feature(*index)).collect();
    neutral::select_variant(
        variants,
        |variant| features(keywords(variant)),
        &features(declared),
        &enabled_features,
        UnknownFeatures::Ignore,
    )
    .map_err(|_| VariantError::NoMatch {
        wanted: enabled
            .iter()
            .copied()
            .filter(|keyword| declared.contains(keyword))
            .collect(),
        available: variants.len(),
    })
}

/// A keyword index as the feature it stands for.
fn feature(index: u16) -> FeatureId {
    FeatureId::new(index.to_string())
}

/// The keywords a program's variants differ by: its keyword state mask, or when that is empty
/// the keywords its variants use.
#[must_use]
pub fn declared_keywords(program: &Program) -> Vec<u16> {
    if !program.keyword_state_mask.is_empty() {
        return program.keyword_state_mask.clone();
    }
    let used: BTreeSet<u16> = program
        .sub_programs
        .iter()
        .map(|v| v.keyword_indices.as_slice())
        .chain(
            program
                .player_sub_programs
                .iter()
                .flatten()
                .map(|v| v.keyword_indices.as_slice()),
        )
        .flatten()
        .copied()
        .collect();
    used.into_iter().collect()
}

/// The [`SubProgram`] of a program for the enabled keywords.
///
/// # Errors
/// Fails when no variant matches.
pub fn select_sub_program<'a>(
    program: &'a Program,
    enabled: &BTreeSet<u16>,
) -> Result<&'a SubProgram, VariantError> {
    select_variant(
        &program.sub_programs,
        |v| &v.keyword_indices,
        &declared_keywords(program),
        enabled,
    )
}

/// The [`PlayerSubProgram`] for the enabled keywords, searching every platform list in order.
///
/// # Errors
/// Fails when no variant matches.
pub fn select_player_sub_program<'a>(
    program: &'a Program,
    enabled: &BTreeSet<u16>,
) -> Result<&'a PlayerSubProgram, VariantError> {
    let all: Vec<&PlayerSubProgram> = program.player_sub_programs.iter().flatten().collect();
    select_variant(
        &all,
        |v| &v.keyword_indices,
        &declared_keywords(program),
        enabled,
    )
    .copied()
}
