use crate::{ImportError, ImportProfile, ImportSidecar, ImportedScene, source_io::error};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use worldrunner_asset_id::AssetVariant;
use worldrunner_package::AssetInput;

/// Owned read bundle. No native objects, simulation state or filesystem-backed buffers.
pub struct ContentReceipt {
    graph: ImportedScene,
    sources: BTreeMap<String, Vec<u8>>,
    profile: ImportProfile,
    sidecar: ImportSidecar,
    closure_sha256: String,
    dependency_references: Vec<crate::LocatedReference>,
}
impl ContentReceipt {
    pub fn graph(&self) -> &ImportedScene {
        &self.graph
    }
    pub fn sources(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.sources
    }
    pub fn closure_sha256(&self) -> &str {
        &self.closure_sha256
    }
    pub fn dependency_references(&self) -> &[crate::LocatedReference] {
        &self.dependency_references
    }
    pub fn proof(&self) -> Value {
        json!({"schema_version":1,"importer":"worldrunner-compat-content","importer_version":env!("CARGO_PKG_VERSION"),"scene":self.graph.scene,
            "closure_sha256":self.closure_sha256,"source_hashes":self.graph.source_hashes,
            "profile":self.profile,"sidecar":self.sidecar,"graph":self.graph,"dependency_references":self.dependency_references})
    }
    /// Reimport against current inputs before export/adoption. Future imports remain valid.
    pub fn verify_current(&self, root: &Path) -> Result<(), ImportError> {
        let current = import_receipt(
            root,
            Path::new(&self.graph.scene),
            &self.profile,
            &self.sidecar,
        )?;
        if self.closure_sha256 != current.closure_sha256 {
            return Err(error(
                &self.graph.scene,
                "content closure changed since read; reimport current sources",
            ));
        }
        Ok(())
    }
    pub fn package_current(
        &self,
        root: &Path,
        variant: &AssetVariant,
        source_package: &str,
    ) -> Result<Vec<AssetInput>, ImportError> {
        self.verify_current(root)?;
        self.graph
            .package_inputs(self.sources.clone(), variant, source_package)
    }
}
pub fn import_receipt(
    root: &Path,
    scene: &Path,
    profile: &ImportProfile,
    sidecar: &ImportSidecar,
) -> Result<ContentReceipt, ImportError> {
    import_receipt_with_source(root, scene, profile, sidecar, &crate::FileContentSource)
}

pub fn import_receipt_with_source(
    root: &Path,
    scene: &Path,
    profile: &ImportProfile,
    sidecar: &ImportSidecar,
    source: &dyn crate::ContentSource,
) -> Result<ContentReceipt, ImportError> {
    let (graph, sources) = crate::import_inner(root, scene, profile, sidecar, Some(source))?;
    let identity = serde_json::to_vec(&(
        env!("CARGO_PKG_VERSION"),
        &graph.scene,
        &graph.scene_guid,
        &graph.source_hashes,
        profile,
        sidecar,
    ))
    .map_err(|e| error(&graph.scene, e.to_string()))?;
    let mut receipt = ContentReceipt {
        graph,
        sources,
        profile: profile.clone(),
        sidecar: sidecar.clone(),
        closure_sha256: format!("{:x}", Sha256::digest(identity)),
        dependency_references: Vec::new(),
    };
    receipt.dependency_references = crate::dependency::observe(&receipt)?;
    Ok(receipt)
}
