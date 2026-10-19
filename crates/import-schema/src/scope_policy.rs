use crate::validation::{is_identifier, is_nonempty_text};
use crate::version::{
    ENVIRONMENT_SCOPE_POLICY_SCHEMA, ENVIRONMENT_SCOPE_POLICY_VERSION, VersionedDocumentError,
    parse_versioned, to_canonical_vec, to_pretty_vec, verify_header,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

/// The class match performed by a scope rule.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScopeSelector {
    RootClass { class: String },
    ExactClass { class: String },
}

impl ScopeSelector {
    fn class(&self) -> &str {
        match self {
            Self::RootClass { class } | Self::ExactClass { class } => class,
        }
    }
}

/// Policy disposition for a source class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeDisposition {
    ExcludedByDesign,
    Withheld,
    Environmental,
}

/// A cited, stable scope decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeRule {
    pub id: String,
    pub selector: ScopeSelector,
    pub disposition: ScopeDisposition,
    pub reason: String,
    pub evidence: String,
    pub descendants_inherit: bool,
}

/// Whether a resolved rule matched the current class or an ancestor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchKind {
    Exact,
    Root,
    Inherited,
}

/// Versioned scope policy shared by audit, import, census, and runtime checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentScopePolicy {
    pub schema: String,
    pub schema_version: u32,
    pub policy_id: String,
    pub agreed_exclusion_roots: Vec<String>,
    pub rules: Vec<ScopeRule>,
}

impl EnvironmentScopePolicy {
    /// Parses and validates a scope policy.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, or semantic policy error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ScopePolicyError> {
        let policy: Self = parse_versioned(
            bytes,
            ENVIRONMENT_SCOPE_POLICY_SCHEMA,
            ENVIRONMENT_SCOPE_POLICY_VERSION,
        )?;
        policy.validate()?;
        Ok(policy)
    }

    /// Serializes this policy as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the policy is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, ScopePolicyError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this policy as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the policy is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, ScopePolicyError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Validates envelope, ordering, rule uniqueness, citations, and exclusion boundaries.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed policy violation.
    pub fn validate(&self) -> Result<(), ScopePolicyError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            ENVIRONMENT_SCOPE_POLICY_SCHEMA,
            ENVIRONMENT_SCOPE_POLICY_VERSION,
        )?;
        if !is_identifier(&self.policy_id) {
            return Err(ScopePolicyError::InvalidPolicyId(self.policy_id.clone()));
        }
        validate_agreed_roots(&self.agreed_exclusion_roots)?;
        validate_rules(self)?;
        validate_root_coverage(self)?;
        Ok(())
    }

    /// Resolves a class ancestry ordered from root to current class.
    ///
    /// Resolution is fail-closed: the ancestry must be nonempty, every class must satisfy the
    /// universal identifier grammar with no duplicates, and some rule must classify the
    /// ancestry. There is no wildcard, default, or fallback disposition.
    ///
    /// # Errors
    ///
    /// Returns [`ScopeResolutionError`] describing the first violated ancestry contract or
    /// the unclassified ancestry.
    pub fn resolve<'a>(
        &'a self,
        ancestry: &[&str],
    ) -> Result<(&'a ScopeRule, MatchKind), ScopeResolutionError> {
        let Some(current) = ancestry.last().copied() else {
            return Err(ScopeResolutionError::EmptyAncestry);
        };
        validate_class_ancestry(ancestry)?;
        if let Some(rule) = self.rules.iter().find(|rule| {
            matches!(&rule.selector, ScopeSelector::ExactClass { class } if class == current)
        }) {
            return Ok((rule, MatchKind::Exact));
        }
        ancestry
            .iter()
            .enumerate()
            .rev()
            .find_map(|(depth, class)| {
                self.rules.iter().find_map(|rule| match &rule.selector {
                    ScopeSelector::RootClass { class: root } if root == class => {
                        root_match(rule, depth, ancestry.len())
                    }
                    ScopeSelector::ExactClass { .. } | ScopeSelector::RootClass { .. } => None,
                })
            })
            .ok_or_else(|| ScopeResolutionError::UnclassifiedAncestry {
                class: current.to_owned(),
            })
    }

    /// Finds a rule by its stable policy identifier.
    pub fn rule(&self, id: &str) -> Option<&ScopeRule> {
        self.rules.iter().find(|rule| rule.id == id)
    }
}

fn root_match(
    rule: &ScopeRule,
    depth: usize,
    ancestry_len: usize,
) -> Option<(&ScopeRule, MatchKind)> {
    if depth + 1 == ancestry_len {
        return Some((rule, MatchKind::Root));
    }
    rule.descendants_inherit
        .then_some((rule, MatchKind::Inherited))
}

/// Validates one root-to-current class ancestry independently of policy coverage.
///
/// # Errors
///
/// Returns an error when the ancestry is empty, malformed, or repeats a class.
pub fn validate_class_ancestry(ancestry: &[&str]) -> Result<(), ScopeResolutionError> {
    if ancestry.is_empty() {
        return Err(ScopeResolutionError::EmptyAncestry);
    }
    let mut seen = BTreeSet::new();
    for class in ancestry {
        if !is_identifier(class) {
            return Err(ScopeResolutionError::InvalidAncestryClass(
                (*class).to_owned(),
            ));
        }
        if !seen.insert(*class) {
            return Err(ScopeResolutionError::DuplicateAncestryClass(
                (*class).to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_agreed_roots(roots: &[String]) -> Result<(), ScopePolicyError> {
    let mut previous: Option<&str> = None;
    for root in roots {
        if !is_identifier(root) {
            return Err(ScopePolicyError::InvalidClass(root.clone()));
        }
        if previous.is_some_and(|value| value >= root.as_str()) {
            return Err(ScopePolicyError::UnsortedExclusionRoots);
        }
        previous = Some(root);
    }
    Ok(())
}

fn validate_rules(policy: &EnvironmentScopePolicy) -> Result<(), ScopePolicyError> {
    let mut ids = BTreeSet::new();
    let mut selectors = BTreeSet::new();
    let mut previous_id: Option<&str> = None;
    for rule in &policy.rules {
        validate_rule(rule, &policy.agreed_exclusion_roots)?;
        if !ids.insert(rule.id.as_str()) {
            return Err(ScopePolicyError::DuplicateRuleId(rule.id.clone()));
        }
        if !selectors.insert(&rule.selector) {
            return Err(ScopePolicyError::DuplicateSelector(rule.selector.clone()));
        }
        if previous_id.is_some_and(|value| value >= rule.id.as_str()) {
            return Err(ScopePolicyError::UnsortedRules);
        }
        previous_id = Some(&rule.id);
    }
    Ok(())
}

fn validate_rule(rule: &ScopeRule, agreed_roots: &[String]) -> Result<(), ScopePolicyError> {
    if !is_identifier(&rule.id) {
        return Err(ScopePolicyError::InvalidRuleId(rule.id.clone()));
    }
    if !is_identifier(rule.selector.class()) {
        return Err(ScopePolicyError::InvalidClass(
            rule.selector.class().to_owned(),
        ));
    }
    if !is_nonempty_text(&rule.reason) || !is_nonempty_text(&rule.evidence) {
        return Err(ScopePolicyError::MissingCitation(rule.id.clone()));
    }
    if matches!(rule.selector, ScopeSelector::ExactClass { .. }) && rule.descendants_inherit {
        return Err(ScopePolicyError::ExactRuleCannotInherit(rule.id.clone()));
    }
    if matches!(rule.selector, ScopeSelector::RootClass { .. }) && !rule.descendants_inherit {
        return Err(ScopePolicyError::RootRuleMustInherit(rule.id.clone()));
    }
    if rule.disposition == ScopeDisposition::ExcludedByDesign
        && !agreed_roots
            .iter()
            .any(|root| root == rule.selector.class())
    {
        return Err(ScopePolicyError::ExclusionOutsideAgreedSet(
            rule.selector.class().to_owned(),
        ));
    }
    Ok(())
}

fn validate_root_coverage(policy: &EnvironmentScopePolicy) -> Result<(), ScopePolicyError> {
    for root in &policy.agreed_exclusion_roots {
        let covered = policy.rules.iter().any(|rule| {
            rule.disposition == ScopeDisposition::ExcludedByDesign
                && matches!(&rule.selector, ScopeSelector::RootClass { class } if class == root)
                && rule.descendants_inherit
        });
        if !covered {
            return Err(ScopePolicyError::UncoveredExclusionRoot(root.clone()));
        }
    }
    Ok(())
}

/// Fail-closed class-ancestry resolution errors.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum ScopeResolutionError {
    #[error("class ancestry is empty")]
    EmptyAncestry,
    #[error("class ancestry contains invalid class identifier {0:?}")]
    InvalidAncestryClass(String),
    #[error("class ancestry repeats class {0:?}")]
    DuplicateAncestryClass(String),
    #[error("class ancestry ending at {class:?} is not classified by the policy")]
    UnclassifiedAncestry { class: String },
}

/// Scope-policy decoding and semantic errors.
#[derive(Debug, Error)]
pub enum ScopePolicyError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error("invalid policy ID {0:?}")]
    InvalidPolicyId(String),
    #[error("invalid rule ID {0:?}")]
    InvalidRuleId(String),
    #[error("invalid class identifier {0:?}")]
    InvalidClass(String),
    #[error("agreed exclusion roots must be strictly sorted and unique")]
    UnsortedExclusionRoots,
    #[error("policy rules must be strictly sorted by ID")]
    UnsortedRules,
    #[error("duplicate policy rule ID {0}")]
    DuplicateRuleId(String),
    #[error("duplicate policy selector {0:?}")]
    DuplicateSelector(ScopeSelector),
    #[error("policy rule {0} requires nonempty reason and evidence")]
    MissingCitation(String),
    #[error("exact-class policy rule {0} cannot be inherited")]
    ExactRuleCannotInherit(String),
    #[error("root-class policy rule {0} must apply to descendants; use exact_class otherwise")]
    RootRuleMustInherit(String),
    #[error("excluded class {0} is outside the agreed gameplay exclusion set")]
    ExclusionOutsideAgreedSet(String),
    #[error("agreed exclusion root {0} lacks an inherited root rule")]
    UncoveredExclusionRoot(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exclusion_rule(id: &str, class: &str) -> ScopeRule {
        ScopeRule {
            id: id.to_owned(),
            selector: ScopeSelector::RootClass {
                class: class.to_owned(),
            },
            disposition: ScopeDisposition::ExcludedByDesign,
            reason: "Outside environment reconstruction scope".to_owned(),
            evidence: "project scope decision".to_owned(),
            descendants_inherit: true,
        }
    }

    fn policy() -> EnvironmentScopePolicy {
        EnvironmentScopePolicy {
            schema: ENVIRONMENT_SCOPE_POLICY_SCHEMA.to_owned(),
            schema_version: ENVIRONMENT_SCOPE_POLICY_VERSION,
            policy_id: "environment-scope".to_owned(),
            agreed_exclusion_roots: vec!["Building".to_owned()],
            rules: vec![exclusion_rule("exclude-building", "Building")],
        }
    }

    #[test]
    fn policy_round_trip_and_inheritance_are_stable() {
        let policy = policy();
        let bytes = policy.to_json_bytes().unwrap();
        assert_eq!(policy.to_json_bytes().unwrap(), bytes);
        let decoded = EnvironmentScopePolicy::from_json(&bytes).unwrap();
        assert_eq!(decoded, policy);
        let (rule, kind) = decoded.resolve(&["Building", "Wall"]).unwrap();
        assert_eq!(rule.id, "exclude-building");
        assert_eq!(kind, MatchKind::Inherited);
    }

    #[test]
    fn exact_rule_overrides_inherited_root() {
        let mut policy = policy();
        policy.rules.push(ScopeRule {
            id: "wall-withheld".to_owned(),
            selector: ScopeSelector::ExactClass {
                class: "Wall".to_owned(),
            },
            disposition: ScopeDisposition::Withheld,
            reason: "Diagnostic exception".to_owned(),
            evidence: "fixture".to_owned(),
            descendants_inherit: false,
        });
        let (rule, kind) = policy.resolve(&["Building", "Wall"]).unwrap();
        assert_eq!(rule.id, "wall-withheld");
        assert_eq!(kind, MatchKind::Exact);
    }

    #[test]
    fn malformed_and_unclassified_ancestries_fail_closed() {
        let policy = policy();
        assert_eq!(
            policy.resolve(&[]),
            Err(ScopeResolutionError::EmptyAncestry)
        );
        assert_eq!(
            policy.resolve(&["bad class"]),
            Err(ScopeResolutionError::InvalidAncestryClass(
                "bad class".to_owned()
            ))
        );
        assert_eq!(
            policy.resolve(&["Building", "Building"]),
            Err(ScopeResolutionError::DuplicateAncestryClass(
                "Building".to_owned()
            ))
        );
        assert_eq!(
            policy.resolve(&["Landscape"]),
            Err(ScopeResolutionError::UnclassifiedAncestry {
                class: "Landscape".to_owned()
            })
        );
    }

    #[test]
    fn duplicate_selector_is_rejected() {
        let mut policy = policy();
        policy
            .rules
            .push(exclusion_rule("exclude-building-again", "Building"));
        assert!(matches!(
            policy.validate(),
            Err(ScopePolicyError::DuplicateSelector(_))
        ));
    }

    #[test]
    fn duplicate_rule_id_is_rejected() {
        let mut policy = policy();
        policy.rules.push(ScopeRule {
            id: "exclude-building".to_owned(),
            selector: ScopeSelector::ExactClass {
                class: "Wall".to_owned(),
            },
            disposition: ScopeDisposition::Environmental,
            reason: "Environment exception".to_owned(),
            evidence: "fixture".to_owned(),
            descendants_inherit: false,
        });
        assert!(matches!(
            policy.validate(),
            Err(ScopePolicyError::DuplicateRuleId(_))
        ));
    }

    #[test]
    fn root_rule_must_apply_to_descendants() {
        let mut policy = policy();
        policy.rules[0].descendants_inherit = false;
        assert!(matches!(
            policy.validate(),
            Err(ScopePolicyError::RootRuleMustInherit(_))
        ));
    }

    #[test]
    fn exclusion_outside_agreed_set_is_rejected() {
        let mut policy = policy();
        policy.rules[0] = exclusion_rule("exclude-cloud", "Cloud");
        assert!(matches!(
            policy.validate(),
            Err(ScopePolicyError::ExclusionOutsideAgreedSet(class)) if class == "Cloud"
        ));
    }

    #[test]
    fn uncovered_agreed_root_is_rejected() {
        let mut policy = policy();
        policy.rules.clear();
        assert!(matches!(
            policy.validate(),
            Err(ScopePolicyError::UncoveredExclusionRoot(root)) if root == "Building"
        ));
    }

    #[test]
    fn unknown_fields_and_future_versions_are_rejected() {
        let future =
            br#"{"schema":"real-disunity.environment-scope-policy.v1","schema_version":2}"#;
        assert!(matches!(
            EnvironmentScopePolicy::from_json(future),
            Err(ScopePolicyError::Document(
                VersionedDocumentError::UnsupportedSchemaVersion { .. }
            ))
        ));
        assert!(matches!(
            EnvironmentScopePolicy::from_json(b"{"),
            Err(ScopePolicyError::Document(VersionedDocumentError::Json(_)))
        ));

        let mut text = String::from_utf8(policy().to_json_bytes().unwrap()).unwrap();
        text = text.replace("\"policy_id\"", "\"unexpected_field\": 1, \"policy_id\"");
        assert!(matches!(
            EnvironmentScopePolicy::from_json(text.as_bytes()),
            Err(ScopePolicyError::Document(VersionedDocumentError::Json(_)))
        ));
    }
}
