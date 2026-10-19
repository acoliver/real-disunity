use crate::validation::{is_identifier, is_uri};
use crate::{EnvironmentScopePolicy, ScopeDisposition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Terminal pre-runtime disposition of one independently decoded source record.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CensusBucket {
    Selected,
    ExcludedByDesign,
    Withheld,
}

impl CensusBucket {
    const ALL: [Self; 3] = [Self::Selected, Self::ExcludedByDesign, Self::Withheld];
}

/// How one decoded source record reached its post-corpus runtime disposition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CensusResolution {
    Direct,
    GraphOutput,
    GraphNoOutput,
}

impl CensusResolution {
    const ALL: [Self; 3] = [Self::Direct, Self::GraphOutput, Self::GraphNoOutput];
}

/// Exact partition for an optional source fact without inventing sentinel identifiers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionalTextPartition {
    pub absent: u64,
    pub present: BTreeMap<String, u64>,
}

impl OptionalTextPartition {
    /// Returns the checked number of records represented by this partition.
    #[must_use]
    pub fn total(&self) -> Option<u64> {
        checked_sum(std::iter::once(self.absent).chain(self.present.values().copied()))
    }

    fn validate_with(
        &self,
        field: &'static str,
        expected: u64,
        valid_key: fn(&str) -> bool,
    ) -> Result<(), CensusSummaryError> {
        if self.present.keys().any(|key| !valid_key(key)) {
            return Err(CensusSummaryError::InvalidPartitionKey { field });
        }
        require_partition(field, self.total(), expected)
    }
}

/// Exact successful import stages earned before runtime or GPU work.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CensusFunnel {
    pub decoded: u64,
    pub classified: u64,
    pub selected: u64,
    pub dependency_closed: u64,
    pub assembled: u64,
}

/// Exact selected resource-binding closure partitioned by authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingClosureSummary {
    pub source_bindings: u64,
    pub corpus_bindings: u64,
}

impl BindingClosureSummary {
    /// Returns the checked total selected binding count.
    #[must_use]
    pub fn total(&self) -> Option<u64> {
        self.source_bindings.checked_add(self.corpus_bindings)
    }
}

/// Importer-earned summary of source classification and selected dependency closure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CensusSummary {
    pub inventory_total: u64,
    pub bucket_totals: BTreeMap<CensusBucket, u64>,
    pub source_entry_totals: BTreeMap<String, u64>,
    pub source_schema_totals: BTreeMap<String, u64>,
    pub source_kind_totals: OptionalTextPartition,
    pub source_state_totals: OptionalTextPartition,
    pub terminal_class_totals: BTreeMap<String, u64>,
    pub policy_rule_totals: BTreeMap<String, u64>,
    pub resolution_totals: BTreeMap<CensusResolution, u64>,
    pub funnel: CensusFunnel,
    pub binding_closure: BindingClosureSummary,
}

impl CensusSummary {
    /// Returns the checked sum of terminal bucket counts.
    #[must_use]
    pub fn bucket_total(&self) -> Option<u64> {
        checked_sum(self.bucket_totals.values().copied())
    }

    /// Validates exact partitions and successful pre-runtime stage equalities.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed summary violation.
    pub fn validate(&self) -> Result<(), CensusSummaryError> {
        require_terminal_buckets(&self.bucket_totals)?;
        require_partition("bucket totals", self.bucket_total(), self.inventory_total)?;
        validate_named_partition(
            "source entry totals",
            &self.source_entry_totals,
            self.inventory_total,
        )?;
        validate_uri_partition(
            "source schema totals",
            &self.source_schema_totals,
            self.inventory_total,
        )?;
        self.source_kind_totals.validate_with(
            "source kind totals",
            self.inventory_total,
            is_uri,
        )?;
        self.source_state_totals.validate_with(
            "source state totals",
            self.inventory_total,
            is_identifier,
        )?;
        validate_named_partition(
            "terminal class totals",
            &self.terminal_class_totals,
            self.inventory_total,
        )?;
        validate_named_partition(
            "policy rule totals",
            &self.policy_rule_totals,
            self.inventory_total,
        )?;
        require_resolution_buckets(&self.resolution_totals)?;
        require_partition(
            "resolution totals",
            checked_sum(self.resolution_totals.values().copied()),
            self.inventory_total,
        )?;
        validate_funnel(self)?;
        self.binding_closure
            .total()
            .ok_or(CensusSummaryError::CountOverflow)?;
        Ok(())
    }

    /// Verifies that rule totals cite this exact policy and reproduce terminal buckets.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown rule or disposition-count mismatch.
    pub fn validate_against_policy(
        &self,
        policy: &EnvironmentScopePolicy,
    ) -> Result<(), CensusSummaryError> {
        self.validate()?;
        let mut earned = zero_bucket_totals();
        for (rule_id, count) in &self.policy_rule_totals {
            let rule = policy
                .rule(rule_id)
                .ok_or_else(|| CensusSummaryError::UnknownPolicyRule(rule_id.clone()))?;
            let bucket = bucket_for_disposition(rule.disposition);
            let total = earned
                .get_mut(&bucket)
                .ok_or(CensusSummaryError::MissingTerminalBucket)?;
            *total = total
                .checked_add(*count)
                .ok_or(CensusSummaryError::CountOverflow)?;
        }
        if earned != self.bucket_totals {
            return Err(CensusSummaryError::PolicyDispositionMismatch);
        }
        Ok(())
    }
}

fn bucket_for_disposition(disposition: ScopeDisposition) -> CensusBucket {
    match disposition {
        ScopeDisposition::Environmental => CensusBucket::Selected,
        ScopeDisposition::ExcludedByDesign => CensusBucket::ExcludedByDesign,
        ScopeDisposition::Withheld => CensusBucket::Withheld,
    }
}

fn zero_bucket_totals() -> BTreeMap<CensusBucket, u64> {
    CensusBucket::ALL
        .into_iter()
        .map(|bucket| (bucket, 0))
        .collect()
}

fn require_terminal_buckets(
    totals: &BTreeMap<CensusBucket, u64>,
) -> Result<(), CensusSummaryError> {
    if CensusBucket::ALL
        .iter()
        .any(|bucket| !totals.contains_key(bucket))
    {
        return Err(CensusSummaryError::MissingTerminalBucket);
    }
    Ok(())
}

fn require_resolution_buckets(
    totals: &BTreeMap<CensusResolution, u64>,
) -> Result<(), CensusSummaryError> {
    if CensusResolution::ALL
        .iter()
        .any(|resolution| !totals.contains_key(resolution))
    {
        return Err(CensusSummaryError::MissingResolutionBucket);
    }
    Ok(())
}

fn validate_named_partition(
    field: &'static str,
    totals: &BTreeMap<String, u64>,
    expected: u64,
) -> Result<(), CensusSummaryError> {
    if totals.keys().any(|key| !is_identifier(key)) {
        return Err(CensusSummaryError::InvalidPartitionKey { field });
    }
    require_partition(field, checked_sum(totals.values().copied()), expected)
}

fn validate_uri_partition(
    field: &'static str,
    totals: &BTreeMap<String, u64>,
    expected: u64,
) -> Result<(), CensusSummaryError> {
    if totals.keys().any(|key| !is_uri(key)) {
        return Err(CensusSummaryError::InvalidPartitionKey { field });
    }
    require_partition(field, checked_sum(totals.values().copied()), expected)
}

fn require_partition(
    field: &'static str,
    observed: Option<u64>,
    expected: u64,
) -> Result<(), CensusSummaryError> {
    let observed = observed.ok_or(CensusSummaryError::CountOverflow)?;
    if observed != expected {
        return Err(CensusSummaryError::PartitionMismatch {
            field,
            expected,
            observed,
        });
    }
    Ok(())
}

fn validate_funnel(summary: &CensusSummary) -> Result<(), CensusSummaryError> {
    let selected = summary
        .bucket_totals
        .get(&CensusBucket::Selected)
        .copied()
        .ok_or(CensusSummaryError::MissingTerminalBucket)?;
    let expected = [
        ("decoded", summary.inventory_total, summary.funnel.decoded),
        (
            "classified",
            summary.inventory_total,
            summary.funnel.classified,
        ),
        ("selected", selected, summary.funnel.selected),
        (
            "dependency_closed",
            selected,
            summary.funnel.dependency_closed,
        ),
        ("assembled", selected, summary.funnel.assembled),
    ];
    for (stage, expected, observed) in expected {
        if observed != expected {
            return Err(CensusSummaryError::FunnelMismatch {
                stage,
                expected,
                observed,
            });
        }
    }
    Ok(())
}

fn checked_sum(mut values: impl Iterator<Item = u64>) -> Option<u64> {
    values.try_fold(0_u64, u64::checked_add)
}

/// Census partition, policy, and exact-stage validation errors.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum CensusSummaryError {
    #[error("all three terminal census buckets must be present")]
    MissingTerminalBucket,
    #[error("all three source-resolution buckets must be present")]
    MissingResolutionBucket,
    #[error("census count arithmetic overflowed")]
    CountOverflow,
    #[error("census {field} partition mismatch: expected {expected}, observed {observed}")]
    PartitionMismatch {
        field: &'static str,
        expected: u64,
        observed: u64,
    },
    #[error("census {field} contains an invalid identifier")]
    InvalidPartitionKey { field: &'static str },
    #[error("census funnel stage {stage} mismatch: expected {expected}, observed {observed}")]
    FunnelMismatch {
        stage: &'static str,
        expected: u64,
        observed: u64,
    },
    #[error("census cites unknown policy rule {0}")]
    UnknownPolicyRule(String),
    #[error("census policy-rule dispositions do not reproduce terminal buckets")]
    PolicyDispositionMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ENVIRONMENT_SCOPE_POLICY_SCHEMA, EnvironmentScopePolicy, ScopeRule, ScopeSelector,
    };

    fn policy() -> EnvironmentScopePolicy {
        EnvironmentScopePolicy {
            schema: ENVIRONMENT_SCOPE_POLICY_SCHEMA.to_owned(),
            schema_version: 1,
            policy_id: "scope-fixture".to_owned(),
            agreed_exclusion_roots: vec!["Excluded".to_owned()],
            rules: vec![
                ScopeRule {
                    id: "exclude".to_owned(),
                    selector: ScopeSelector::RootClass {
                        class: "Excluded".to_owned(),
                    },
                    disposition: ScopeDisposition::ExcludedByDesign,
                    reason: "fixture".to_owned(),
                    evidence: "fixture".to_owned(),
                    descendants_inherit: true,
                },
                ScopeRule {
                    id: "select".to_owned(),
                    selector: ScopeSelector::RootClass {
                        class: "Selected".to_owned(),
                    },
                    disposition: ScopeDisposition::Environmental,
                    reason: "fixture".to_owned(),
                    evidence: "fixture".to_owned(),
                    descendants_inherit: true,
                },
                ScopeRule {
                    id: "withhold".to_owned(),
                    selector: ScopeSelector::RootClass {
                        class: "Withheld".to_owned(),
                    },
                    disposition: ScopeDisposition::Withheld,
                    reason: "fixture".to_owned(),
                    evidence: "fixture".to_owned(),
                    descendants_inherit: true,
                },
            ],
        }
    }

    fn summary() -> CensusSummary {
        CensusSummary {
            inventory_total: 3,
            bucket_totals: BTreeMap::from([
                (CensusBucket::Selected, 1),
                (CensusBucket::ExcludedByDesign, 1),
                (CensusBucket::Withheld, 1),
            ]),
            source_entry_totals: BTreeMap::from([("entry".to_owned(), 3)]),
            source_schema_totals: BTreeMap::from([("urn:fixture:schema:v1".to_owned(), 3)]),
            source_kind_totals: OptionalTextPartition {
                absent: 1,
                present: BTreeMap::from([("urn:fixture:kind:v1".to_owned(), 2)]),
            },
            source_state_totals: OptionalTextPartition {
                absent: 2,
                present: BTreeMap::from([("active".to_owned(), 1)]),
            },
            terminal_class_totals: BTreeMap::from([
                ("Excluded".to_owned(), 1),
                ("Selected".to_owned(), 1),
                ("Withheld".to_owned(), 1),
            ]),
            policy_rule_totals: BTreeMap::from([
                ("exclude".to_owned(), 1),
                ("select".to_owned(), 1),
                ("withhold".to_owned(), 1),
            ]),
            resolution_totals: BTreeMap::from([
                (CensusResolution::Direct, 1),
                (CensusResolution::GraphOutput, 1),
                (CensusResolution::GraphNoOutput, 1),
            ]),
            funnel: CensusFunnel {
                decoded: 3,
                classified: 3,
                selected: 1,
                dependency_closed: 1,
                assembled: 1,
            },
            binding_closure: BindingClosureSummary {
                source_bindings: 1,
                corpus_bindings: 2,
            },
        }
    }

    #[test]
    fn complete_summary_validates_against_exact_policy() {
        let summary = summary();
        assert_eq!(summary.bucket_total(), Some(3));
        assert!(summary.validate_against_policy(&policy()).is_ok());
    }

    #[test]
    fn zero_inventory_is_valid_when_all_partitions_are_explicit() {
        let summary = CensusSummary {
            inventory_total: 0,
            bucket_totals: zero_bucket_totals(),
            source_entry_totals: BTreeMap::from([("empty-entry".to_owned(), 0)]),
            source_schema_totals: BTreeMap::new(),
            source_kind_totals: OptionalTextPartition {
                absent: 0,
                present: BTreeMap::new(),
            },
            source_state_totals: OptionalTextPartition {
                absent: 0,
                present: BTreeMap::new(),
            },
            terminal_class_totals: BTreeMap::new(),
            policy_rule_totals: BTreeMap::new(),
            resolution_totals: BTreeMap::from([
                (CensusResolution::Direct, 0),
                (CensusResolution::GraphOutput, 0),
                (CensusResolution::GraphNoOutput, 0),
            ]),
            funnel: CensusFunnel {
                decoded: 0,
                classified: 0,
                selected: 0,
                dependency_closed: 0,
                assembled: 0,
            },
            binding_closure: BindingClosureSummary {
                source_bindings: 0,
                corpus_bindings: 0,
            },
        };
        assert!(summary.validate_against_policy(&policy()).is_ok());
    }

    #[test]
    fn missing_bucket_partition_and_funnel_mismatches_are_rejected() {
        let mut missing = summary();
        missing.bucket_totals.remove(&CensusBucket::Withheld);
        assert_eq!(
            missing.validate(),
            Err(CensusSummaryError::MissingTerminalBucket)
        );

        let mut partition = summary();
        partition.source_entry_totals.insert("entry".to_owned(), 2);
        assert!(matches!(
            partition.validate(),
            Err(CensusSummaryError::PartitionMismatch {
                field: "source entry totals",
                ..
            })
        ));

        let mut funnel = summary();
        funnel.funnel.assembled = 0;
        assert!(matches!(
            funnel.validate(),
            Err(CensusSummaryError::FunnelMismatch {
                stage: "assembled",
                ..
            })
        ));
    }

    #[test]
    fn source_and_resolution_partitions_fail_closed_independently() {
        let mut missing_resolution = summary();
        missing_resolution
            .resolution_totals
            .remove(&CensusResolution::GraphNoOutput);
        assert_eq!(
            missing_resolution.validate(),
            Err(CensusSummaryError::MissingResolutionBucket)
        );

        let mut kind_partition = summary();
        kind_partition.source_kind_totals.absent = 2;
        assert!(matches!(
            kind_partition.validate(),
            Err(CensusSummaryError::PartitionMismatch {
                field: "source kind totals",
                ..
            })
        ));

        let mut invalid_schema = summary();
        invalid_schema.source_schema_totals.clear();
        invalid_schema
            .source_schema_totals
            .insert("not a uri".to_owned(), 3);
        assert!(matches!(
            invalid_schema.validate(),
            Err(CensusSummaryError::InvalidPartitionKey {
                field: "source schema totals"
            })
        ));

        let mut invalid_state = summary();
        invalid_state.source_state_totals.absent = 3;
        invalid_state.source_state_totals.present.clear();
        invalid_state
            .source_state_totals
            .present
            .insert("bad state".to_owned(), 0);
        assert!(matches!(
            invalid_state.validate(),
            Err(CensusSummaryError::InvalidPartitionKey {
                field: "source state totals"
            })
        ));
    }

    #[test]
    fn unknown_rules_and_wrong_disposition_totals_fail_closed() {
        let mut unknown = summary();
        unknown.policy_rule_totals.remove("select");
        unknown.policy_rule_totals.insert("unknown".to_owned(), 1);
        assert_eq!(
            unknown.validate_against_policy(&policy()),
            Err(CensusSummaryError::UnknownPolicyRule("unknown".to_owned()))
        );

        let mut wrong = summary();
        wrong.policy_rule_totals.insert("exclude".to_owned(), 0);
        wrong.policy_rule_totals.insert("select".to_owned(), 2);
        assert_eq!(
            wrong.validate_against_policy(&policy()),
            Err(CensusSummaryError::PolicyDispositionMismatch)
        );
    }

    #[test]
    fn invalid_keys_and_overflow_are_rejected() {
        let mut invalid = summary();
        invalid.source_entry_totals.insert("bad key".to_owned(), 0);
        assert!(matches!(
            invalid.validate(),
            Err(CensusSummaryError::InvalidPartitionKey {
                field: "source entry totals"
            })
        ));

        let closure = BindingClosureSummary {
            source_bindings: u64::MAX,
            corpus_bindings: 1,
        };
        let mut overflow = summary();
        overflow.binding_closure = closure;
        assert_eq!(overflow.validate(), Err(CensusSummaryError::CountOverflow));
    }
}
