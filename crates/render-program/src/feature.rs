//! Choosing a program's variant from the features that are enabled.
//!
//! A variant is one program built for an exact set of features. Matching is exact: the enabled
//! features, restricted to the ones the variants differ by, must equal a variant's set.

use std::collections::{BTreeMap, BTreeSet};

/// A feature a program's variants differ by, such as a shader keyword.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FeatureId(String);

impl FeatureId {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FeatureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub type FeatureSet = BTreeSet<FeatureId>;

/// What to do with an enabled feature that no variant differs by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownFeatures {
    /// Name the feature in an error.
    Fail,
    /// Leave the feature out of the match.
    Ignore,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum VariantError {
    #[error("no variant of {available} matches the feature set {wanted:?}")]
    NoMatch {
        wanted: Vec<FeatureId>,
        available: usize,
    },
    #[error("feature {name} is not one the variants differ by")]
    UnknownFeature { name: String },
    #[error("two variants have the feature set {features:?}")]
    DuplicateVariant { features: Vec<FeatureId> },
}

/// The enabled features that take part in the match.
fn wanted(
    declared: &FeatureSet,
    enabled: &FeatureSet,
    unknown: UnknownFeatures,
) -> Result<FeatureSet, VariantError> {
    if unknown == UnknownFeatures::Fail
        && let Some(name) = enabled.iter().find(|feature| !declared.contains(feature))
    {
        return Err(VariantError::UnknownFeature {
            name: name.to_string(),
        });
    }
    Ok(enabled.intersection(declared).cloned().collect())
}

/// Programs keyed by the exact feature set each was built for.
#[derive(Clone, Debug, PartialEq)]
pub struct Variants<P> {
    declared: FeatureSet,
    programs: BTreeMap<FeatureSet, P>,
}

impl<P> Variants<P> {
    /// Variants of `declared` features. A variant's set may hold only declared features.
    ///
    /// # Errors
    /// Fails on a feature outside `declared` (named as an unknown feature) and on two
    /// variants with one set.
    pub fn new(
        declared: FeatureSet,
        programs: impl IntoIterator<Item = (FeatureSet, P)>,
    ) -> Result<Self, VariantError> {
        let mut keyed: BTreeMap<FeatureSet, P> = BTreeMap::new();
        for (features, program) in programs {
            wanted(&declared, &features, UnknownFeatures::Fail)?;
            if keyed.contains_key(&features) {
                return Err(VariantError::DuplicateVariant {
                    features: features.into_iter().collect(),
                });
            }
            keyed.insert(features, program);
        }
        Ok(Self {
            declared,
            programs: keyed,
        })
    }

    #[must_use]
    pub fn declared(&self) -> &FeatureSet {
        &self.declared
    }

    /// The program for the exact feature set `enabled` makes.
    ///
    /// # Errors
    /// Fails on an unknown feature when `unknown` is [`UnknownFeatures::Fail`], and when no
    /// variant has the resulting set.
    pub fn select(
        &self,
        enabled: &FeatureSet,
        unknown: UnknownFeatures,
    ) -> Result<&P, VariantError> {
        let wanted = wanted(&self.declared, enabled, unknown)?;
        self.programs
            .get(&wanted)
            .ok_or_else(|| VariantError::NoMatch {
                wanted: wanted.into_iter().collect(),
                available: self.programs.len(),
            })
    }
}

/// The first of `variants` whose feature set equals the enabled features that `declared`
/// contains, for a caller whose variants are a list of its own records.
///
/// # Errors
/// Fails like [`Variants::select`].
pub fn select_variant<'a, V>(
    variants: &'a [V],
    features: impl Fn(&V) -> FeatureSet,
    declared: &FeatureSet,
    enabled: &FeatureSet,
    unknown: UnknownFeatures,
) -> Result<&'a V, VariantError> {
    let wanted = wanted(declared, enabled, unknown)?;
    variants
        .iter()
        .find(|variant| features(variant) == wanted)
        .ok_or_else(|| VariantError::NoMatch {
            wanted: wanted.into_iter().collect(),
            available: variants.len(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(names: &[&str]) -> FeatureSet {
        names.iter().map(|name| FeatureId::new(*name)).collect()
    }

    fn variants() -> Variants<char> {
        Variants::new(
            set(&["FOG", "SHADOWS"]),
            [
                (set(&[]), 'a'),
                (set(&["FOG"]), 'b'),
                (set(&["FOG", "SHADOWS"]), 'c'),
            ],
        )
        .unwrap()
    }

    #[test]
    fn the_exact_feature_set_selects_its_variant() {
        let variants = variants();
        let pick = |names: &[&str]| *variants.select(&set(names), UnknownFeatures::Fail).unwrap();
        assert_eq!(pick(&[]), 'a');
        assert_eq!(pick(&["FOG"]), 'b');
        assert_eq!(pick(&["SHADOWS", "FOG"]), 'c');
    }

    #[test]
    fn a_set_no_variant_has_is_a_named_error() {
        let error = variants()
            .select(&set(&["SHADOWS"]), UnknownFeatures::Fail)
            .unwrap_err();
        assert_eq!(
            error,
            VariantError::NoMatch {
                wanted: vec![FeatureId::new("SHADOWS")],
                available: 3
            }
        );
    }

    #[test]
    fn an_unknown_feature_fails_by_name_unless_the_caller_ignores_unknowns() {
        let variants = variants();
        assert_eq!(
            variants
                .select(&set(&["FOG", "BLOOM"]), UnknownFeatures::Fail)
                .unwrap_err(),
            VariantError::UnknownFeature {
                name: "BLOOM".to_owned()
            }
        );
        let picked = variants
            .select(&set(&["FOG", "BLOOM"]), UnknownFeatures::Ignore)
            .unwrap();
        assert_eq!(*picked, 'b');
    }

    #[test]
    fn variants_with_an_undeclared_feature_or_a_repeated_set_are_refused() {
        let undeclared = Variants::new(set(&["FOG"]), [(set(&["RAIN"]), 'a')]).unwrap_err();
        assert_eq!(
            undeclared,
            VariantError::UnknownFeature {
                name: "RAIN".to_owned()
            }
        );
        let repeated =
            Variants::new(set(&["FOG"]), [(set(&["FOG"]), 'a'), (set(&["FOG"]), 'b')]).unwrap_err();
        assert_eq!(
            repeated,
            VariantError::DuplicateVariant {
                features: vec![FeatureId::new("FOG")]
            }
        );
    }

    #[test]
    fn a_list_of_records_is_searched_in_order_for_the_first_exact_match() {
        let records = [(set(&[]), 1), (set(&["FOG"]), 2), (set(&["FOG"]), 3)];
        let picked = select_variant(
            &records,
            |record| record.0.clone(),
            &set(&["FOG"]),
            &set(&["FOG"]),
            UnknownFeatures::Ignore,
        )
        .unwrap();
        assert_eq!(picked.1, 2);
        assert!(
            select_variant(
                &records[..0],
                |record: &(FeatureSet, i32)| record.0.clone(),
                &set(&[]),
                &set(&[]),
                UnknownFeatures::Ignore
            )
            .is_err()
        );
    }
}
