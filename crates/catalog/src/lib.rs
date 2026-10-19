use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use worldrunner_asset_id::{ArtifactKind, ContentHash, LogicalAssetId, QualityTier};
use worldrunner_package::{AssetKey, PackageEntry, PackageManifest};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionPolicy {
    pub art_sets: Vec<String>,
    pub platform_profiles: Vec<String>,
    pub maximum_quality: QualityTier,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedAsset {
    pub package_id: String,
    pub package_priority: i32,
    pub key: AssetKey,
    pub kind: ArtifactKind,
    pub content_hash: ContentHash,
    pub dependencies: Vec<LogicalAssetId>,
}

#[derive(Default)]
pub struct CatalogBuilder {
    packages: BTreeSet<String>,
    candidates: BTreeMap<LogicalAssetId, Vec<ResolvedAsset>>,
}

impl CatalogBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mount(
        &mut self,
        manifest: &PackageManifest,
        entries: &[PackageEntry],
        priority: i32,
    ) -> Result<(), CatalogError> {
        if manifest.package_id.is_empty() {
            return Err(CatalogError::EmptyPackageId);
        }
        if !self.packages.insert(manifest.package_id.clone()) {
            return Err(CatalogError::DuplicatePackage(manifest.package_id.clone()));
        }
        for entry in entries {
            self.candidates
                .entry(entry.key.logical_id.clone())
                .or_default()
                .push(ResolvedAsset {
                    package_id: manifest.package_id.clone(),
                    package_priority: priority,
                    key: entry.key.clone(),
                    kind: entry.kind,
                    content_hash: entry.content_hash,
                    dependencies: entry.dependencies.clone(),
                });
        }
        Ok(())
    }

    pub fn build(mut self) -> Catalog {
        for candidates in self.candidates.values_mut() {
            candidates.sort_by(|left, right| {
                (&left.key.variant, left.package_priority, &left.package_id).cmp(&(
                    &right.key.variant,
                    right.package_priority,
                    &right.package_id,
                ))
            });
        }
        Catalog {
            candidates: self.candidates,
        }
    }
}

pub struct Catalog {
    candidates: BTreeMap<LogicalAssetId, Vec<ResolvedAsset>>,
}

impl Catalog {
    pub fn resolve(
        &self,
        logical_id: &LogicalAssetId,
        policy: &ResolutionPolicy,
    ) -> Result<ResolvedAsset, CatalogError> {
        let candidates = self
            .candidates
            .get(logical_id)
            .ok_or_else(|| CatalogError::AssetNotFound(logical_id.clone()))?;
        let mut compatible = candidates
            .iter()
            .filter_map(|candidate| rank(candidate, policy).map(|rank| (rank, candidate)))
            .collect::<Vec<_>>();
        compatible.sort_by(|(left_rank, left), (right_rank, right)| {
            compare_rank(left_rank, right_rank).then_with(|| left.package_id.cmp(&right.package_id))
        });
        let Some((best_rank, best)) = compatible.first() else {
            return Err(CatalogError::NoCompatibleVariant(logical_id.clone()));
        };
        if compatible
            .get(1)
            .is_some_and(|(second_rank, _)| second_rank == best_rank)
        {
            return Err(CatalogError::AmbiguousVariant(logical_id.clone()));
        }
        Ok((*best).clone())
    }

    pub fn resolve_dependency_order(
        &self,
        root: &LogicalAssetId,
        policy: &ResolutionPolicy,
    ) -> Result<Vec<ResolvedAsset>, CatalogError> {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut ordered = Vec::new();
        self.visit(root, policy, &mut visiting, &mut visited, &mut ordered)?;
        Ok(ordered)
    }

    fn visit(
        &self,
        logical_id: &LogicalAssetId,
        policy: &ResolutionPolicy,
        visiting: &mut BTreeSet<LogicalAssetId>,
        visited: &mut BTreeSet<LogicalAssetId>,
        ordered: &mut Vec<ResolvedAsset>,
    ) -> Result<(), CatalogError> {
        if visited.contains(logical_id) {
            return Ok(());
        }
        if !visiting.insert(logical_id.clone()) {
            return Err(CatalogError::DependencyCycle(logical_id.clone()));
        }
        let resolved = self.resolve(logical_id, policy)?;
        for dependency in &resolved.dependencies {
            self.visit(dependency, policy, visiting, visited, ordered)?;
        }
        visiting.remove(logical_id);
        visited.insert(logical_id.clone());
        ordered.push(resolved);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CandidateRank {
    art_set: usize,
    platform_profile: usize,
    quality: QualityTier,
    package_priority: i32,
}

fn rank(candidate: &ResolvedAsset, policy: &ResolutionPolicy) -> Option<CandidateRank> {
    let art_set = policy
        .art_sets
        .iter()
        .position(|item| item == &candidate.key.variant.art_set)?;
    let platform_profile = policy
        .platform_profiles
        .iter()
        .position(|item| item == &candidate.key.variant.platform_profile)?;
    if candidate.key.variant.quality > policy.maximum_quality {
        return None;
    }
    Some(CandidateRank {
        art_set,
        platform_profile,
        quality: candidate.key.variant.quality,
        package_priority: candidate.package_priority,
    })
}

fn compare_rank(left: &CandidateRank, right: &CandidateRank) -> Ordering {
    left.art_set
        .cmp(&right.art_set)
        .then_with(|| left.platform_profile.cmp(&right.platform_profile))
        .then_with(|| right.quality.cmp(&left.quality))
        .then_with(|| right.package_priority.cmp(&left.package_priority))
}

#[derive(Debug, Error, PartialEq)]
pub enum CatalogError {
    #[error("package ID must not be empty")]
    EmptyPackageId,
    #[error("package {0} was mounted more than once")]
    DuplicatePackage(String),
    #[error("asset {0} is not present in the catalog")]
    AssetNotFound(LogicalAssetId),
    #[error("asset {0} has no variant compatible with the resolution policy")]
    NoCompatibleVariant(LogicalAssetId),
    #[error("asset {0} has multiple equally ranked variants")]
    AmbiguousVariant(LogicalAssetId),
    #[error("asset dependency graph contains a cycle at {0}")]
    DependencyCycle(LogicalAssetId),
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_asset_id::{AssetVariant, ProvenanceClassification, SourceProvenance};
    use worldrunner_package::{PackageKind, PackageManifest};

    fn id(value: &str) -> LogicalAssetId {
        LogicalAssetId::new(value).unwrap()
    }

    fn manifest(package_id: &str) -> PackageManifest {
        PackageManifest {
            package_id: package_id.to_owned(),
            semantic_version: "1.0.0".to_owned(),
            package_kind: PackageKind::Development,
            content_schema: 1,
        }
    }

    fn entry(
        logical_id: &str,
        art_set: &str,
        quality: QualityTier,
        dependencies: &[&str],
    ) -> PackageEntry {
        PackageEntry {
            key: AssetKey {
                logical_id: id(logical_id),
                variant: AssetVariant {
                    art_set: art_set.to_owned(),
                    quality,
                    platform_profile: "portable".to_owned(),
                },
            },
            kind: ArtifactKind::Data,
            content_hash: ContentHash::of(logical_id.as_bytes()),
            uncompressed_length: 1,
            chunks: Vec::new(),
            dependencies: dependencies.iter().map(|value| id(value)).collect(),
            provenance: SourceProvenance {
                classification: ProvenanceClassification::Original,
                source_package: "test".to_owned(),
                source_virtual_path: logical_id.to_owned(),
                source_sha256: "00".repeat(32),
                importer_id: "test".to_owned(),
                importer_version: "1".to_owned(),
            },
        }
    }

    fn policy(maximum_quality: QualityTier) -> ResolutionPolicy {
        ResolutionPolicy {
            art_sets: vec!["upgraded".to_owned(), "legacy".to_owned()],
            platform_profiles: vec!["portable".to_owned()],
            maximum_quality,
        }
    }

    #[test]
    fn prefers_art_set_then_highest_allowed_quality_then_package_priority() {
        let mut builder = CatalogBuilder::new();
        builder
            .mount(
                &manifest("base"),
                &[
                    entry("test.asset", "legacy", QualityTier::Baseline, &[]),
                    entry("test.asset", "upgraded", QualityTier::Baseline, &[]),
                ],
                0,
            )
            .unwrap();
        builder
            .mount(
                &manifest("patch"),
                &[entry("test.asset", "upgraded", QualityTier::High, &[])],
                10,
            )
            .unwrap();
        let catalog = builder.build();
        let resolved = catalog
            .resolve(&id("test.asset"), &policy(QualityTier::High))
            .unwrap();
        assert_eq!(resolved.package_id, "patch");
        assert_eq!(resolved.key.variant.quality, QualityTier::High);
    }

    #[test]
    fn falls_back_to_available_quality() {
        let mut builder = CatalogBuilder::new();
        builder
            .mount(
                &manifest("base"),
                &[entry("test.asset", "legacy", QualityTier::Baseline, &[])],
                0,
            )
            .unwrap();
        let resolved = builder
            .build()
            .resolve(&id("test.asset"), &policy(QualityTier::Ultra))
            .unwrap();
        assert_eq!(resolved.key.variant.quality, QualityTier::Baseline);
    }

    #[test]
    fn dependency_plan_is_stable_and_dependency_first() {
        let mut builder = CatalogBuilder::new();
        builder
            .mount(
                &manifest("base"),
                &[
                    entry(
                        "test.material",
                        "legacy",
                        QualityTier::Baseline,
                        &["test.albedo", "test.normal"],
                    ),
                    entry("test.albedo", "legacy", QualityTier::Baseline, &[]),
                    entry("test.normal", "legacy", QualityTier::Baseline, &[]),
                ],
                0,
            )
            .unwrap();
        let order = builder
            .build()
            .resolve_dependency_order(&id("test.material"), &policy(QualityTier::Baseline))
            .unwrap();
        assert_eq!(
            order
                .iter()
                .map(|asset| asset.key.logical_id.as_str())
                .collect::<Vec<_>>(),
            vec!["test.albedo", "test.normal", "test.material"]
        );
    }

    #[test]
    fn equal_priority_override_is_rejected_as_ambiguous() {
        let mut builder = CatalogBuilder::new();
        builder
            .mount(
                &manifest("left"),
                &[entry("test.asset", "legacy", QualityTier::Baseline, &[])],
                0,
            )
            .unwrap();
        builder
            .mount(
                &manifest("right"),
                &[entry("test.asset", "legacy", QualityTier::Baseline, &[])],
                0,
            )
            .unwrap();
        assert_eq!(
            builder
                .build()
                .resolve(&id("test.asset"), &policy(QualityTier::Baseline)),
            Err(CatalogError::AmbiguousVariant(id("test.asset")))
        );
    }
}
