//! Choosing a program's compiled variant from the enabled keywords.

use std::collections::BTreeSet;

use worldrunner_unity_serialized::shader_program::PlayerSubProgram;

use crate::variant::{self as neutral, FeatureId, FeatureSet, UnknownFeatures};

/// No variant matches the enabled keywords.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VariantError {
    #[error("no variant of {available} matches the keyword set {wanted:?}")]
    NoMatch { wanted: Vec<u16>, available: usize },
}

/// The variant whose keyword set equals the enabled keywords the stage declares, together with
/// the default of each required group the enabled keywords leave out (see [`group_defaults`]). A
/// keyword the stage does not declare is ignored.
///
/// When a variant has exactly the enabled set, every required group already has an enabled
/// member, since each variant holds one member of every such group, so the defaults add nothing
/// and that variant is the one chosen.
///
/// # Errors
/// Fails when no variant has the completed keyword set. The error names the enabled keywords the
/// stage declares, as they were passed.
pub fn select_variant<'a, V>(
    variants: &'a [V],
    keywords: impl Fn(&V) -> &[u16],
    declared: &[u16],
    enabled: &BTreeSet<u16>,
) -> Result<&'a V, VariantError> {
    let sets: Vec<&[u16]> = variants.iter().map(&keywords).collect();
    let completed: BTreeSet<u16> = enabled
        .union(&group_defaults(&sets, enabled))
        .copied()
        .collect();
    let features =
        |indices: &[u16]| -> FeatureSet { indices.iter().map(|index| feature(*index)).collect() };
    let enabled_features: FeatureSet = completed.iter().map(|index| feature(*index)).collect();
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
