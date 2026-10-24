use crate::{AssetKind, ImportError, ImportedScene};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use worldrunner_asset_id::{
    ArtifactKind, AssetVariant, LogicalAssetId, ProvenanceClassification, SourceProvenance,
};
use worldrunner_package::AssetInput;

impl ImportedScene {
    /// Produces canonical package-builder inputs from the exact admitted sources.
    /// Reimport changed sources before packaging; hashes never prohibit a fresh import.
    pub fn package_inputs(
        &self,
        mut sources: BTreeMap<String, Vec<u8>>,
        variant: &AssetVariant,
        source_package: &str,
    ) -> Result<Vec<AssetInput>, ImportError> {
        for (path, hash) in &self.source_hashes {
            if sources
                .get(path)
                .is_none_or(|b| format!("{:x}", Sha256::digest(b)) != *hash)
            {
                return Err(ImportError::Source {
                    path: path.clone(),
                    message: "packaging snapshot is stale or missing; reimport current sources"
                        .into(),
                });
            }
        }
        let mut inputs = Vec::new();
        for asset in &self.assets {
            let bytes = sources
                .remove(&asset.path)
                .ok_or_else(|| ImportError::Source {
                    path: asset.path.clone(),
                    message: "asset payload missing".into(),
                })?;
            inputs.push(AssetInput {
                logical_id: asset.logical_id.clone(),
                variant: variant.clone(),
                kind: artifact(&asset.kind),
                dependencies: self.dependencies(&asset.guid),
                provenance: provenance(source_package, &asset.path, &asset.sha256),
                bytes,
            });
        }
        let bytes = serde_json::to_vec(self).map_err(|e| ImportError::Source {
            path: self.scene.clone(),
            message: e.to_string(),
        })?;
        let id = LogicalAssetId::new(format!("unity.{}.graph", self.scene_guid)).map_err(|e| {
            ImportError::Source {
                path: self.scene.clone(),
                message: e.to_string(),
            }
        })?;
        inputs.push(AssetInput {
            logical_id: id,
            variant: variant.clone(),
            kind: ArtifactKind::Data,
            dependencies: self.assets.iter().map(|a| a.logical_id.clone()).collect(),
            provenance: provenance(
                source_package,
                &self.scene,
                &self.source_hashes[&self.scene],
            ),
            bytes,
        });
        Ok(inputs)
    }
    fn dependencies(&self, guid: &str) -> Vec<LogicalAssetId> {
        let mut guids = BTreeSet::new();
        for doc in self
            .documents
            .iter()
            .filter(|d| d.source.guid == guid && d.source.instance.is_empty())
        {
            collect(&doc.fields, &mut guids);
        }
        if let Some(asset) = self.assets.iter().find(|a| a.guid == guid) {
            collect(&asset.metadata, &mut guids);
        }
        self.assets
            .iter()
            .filter(|a| a.guid != guid && guids.contains(&a.guid))
            .map(|a| a.logical_id.clone())
            .collect()
    }
}
fn collect(value: &Value, guids: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(guid) = map.get("guid").and_then(Value::as_str) {
                guids.insert(guid.into());
            }
            for value in map.values() {
                collect(value, guids);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect(value, guids);
            }
        }
        _ => {}
    }
}
fn provenance(package: &str, path: &str, sha256: &str) -> SourceProvenance {
    SourceProvenance {
        classification: ProvenanceClassification::ThirdParty,
        source_package: package.into(),
        source_virtual_path: path.into(),
        source_sha256: sha256.into(),
        importer_id: "worldrunner-compat-content".into(),
        importer_version: env!("CARGO_PKG_VERSION").into(),
    }
}
fn artifact(kind: &AssetKind) -> ArtifactKind {
    match kind {
        AssetKind::Texture => ArtifactKind::Texture,
        AssetKind::Material => ArtifactKind::Material,
        AssetKind::Mesh => ArtifactKind::Mesh,
        AssetKind::Audio => ArtifactKind::Audio,
        _ => ArtifactKind::Data,
    }
}
