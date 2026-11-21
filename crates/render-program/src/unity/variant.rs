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
/// When no variant has that set, the enabled keywords are completed with the default of each
/// required group (see [`group_defaults`]) and matched once more. The completed set is only
/// tried after the exact one fails, so a set that has a variant never changes.
///
/// # Errors
/// Fails when no variant has exactly that keyword set, completed or not. The error names the
/// enabled keywords as they were passed.
pub fn select_variant<'a, V>(
    variants: &'a [V],
    keywords: impl Fn(&V) -> &[u16],
    declared: &[u16],
    enabled: &BTreeSet<u16>,
) -> Result<&'a V, VariantError> {
    let error = match select_exact(variants, &keywords, declared, enabled) {
        Ok(variant) => return Ok(variant),
        Err(error) => error,
    };
    let sets: Vec<&[u16]> = variants.iter().map(&keywords).collect();
    let defaults = group_defaults(&sets, enabled);
    if defaults.is_empty() {
        return Err(error);
    }
    let completed: BTreeSet<u16> = enabled.union(&defaults).copied().collect();
    select_exact(variants, &keywords, declared, &completed).map_err(|_| error)
}

/// The variant whose keyword set equals the enabled keywords the stage declares.
fn select_exact<'a, V>(
    variants: &'a [V],
    keywords: &impl Fn(&V) -> &[u16],
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

/// The first keyword of each required group that none of the `enabled` keywords belongs to.
///
/// A required group is two or more keywords that never occur together and of which every
/// variant has exactly one, such as `_MAIN_LIGHT_SHADOWS` and `_MAIN_LIGHT_SHADOWS_CASCADE` in
/// Old World's terrain passes. No variant lacks them, so a draw that enables none of the group
/// has no exact variant. The player then takes the group's first keyword, the one with the
/// lowest index (declaration order). The variant lists agree: in each pass of that player that
/// has such a group and 128 or more variants, the last variant of the list holds the first
/// keyword of each group and no other keyword. A keyword that every variant carries is a group
/// of one and a choice nobody makes, so it gets no default, and a group some variant lacks has
/// an off state of its own.
fn group_defaults(sets: &[&[u16]], enabled: &BTreeSet<u16>) -> BTreeSet<u16> {
    let Some(first) = sets.first() else {
        return BTreeSet::new();
    };
    let declared: BTreeSet<u16> = sets.iter().flat_map(|set| set.iter().copied()).collect();
    first
        .iter()
        .filter_map(|&keyword| required_group(sets, &declared, keyword))
        .filter(|group| group.is_disjoint(enabled))
        .filter_map(|group| group.first().copied())
        .collect()
}

/// The required group `keyword` belongs to: `keyword` and every keyword that no variant has
/// together with it, when each variant has exactly one of them.
fn required_group(
    sets: &[&[u16]],
    declared: &BTreeSet<u16>,
    keyword: u16,
) -> Option<BTreeSet<u16>> {
    let together: BTreeSet<u16> = sets
        .iter()
        .filter(|set| set.contains(&keyword))
        .flat_map(|set| set.iter().copied())
        .collect();
    let mut group: BTreeSet<u16> = declared.difference(&together).copied().collect();
    group.insert(keyword);
    let one_each = sets
        .iter()
        .all(|set| set.iter().filter(|member| group.contains(member)).count() == 1);
    (group.len() > 1 && one_each).then_some(group)
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

/// The keywords the player variants differ by: the ones they use.
///
/// The Unity 6 player stores every Vulkan variant of a pass on its vertex program, and that
/// program's state mask names stereo keywords that no variant uses while the variants differ by
/// fragment keywords. Declaring the mask's keywords would match every enabled keyword set against
/// the empty one and always take the variant with no keywords.
#[must_use]
pub fn declared_player_keywords<'a>(
    variants: impl IntoIterator<Item = &'a PlayerSubProgram>,
) -> Vec<u16> {
    let used: BTreeSet<u16> = variants
        .into_iter()
        .flat_map(|variant| variant.keyword_indices.iter().copied())
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
        &declared_player_keywords(all.iter().copied()),
        enabled,
    )
    .copied()
}
