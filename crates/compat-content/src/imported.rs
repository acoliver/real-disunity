use crate::{ImportError, ImportProfile, ImportSidecar, ImportedScene, source_io::error};
use std::{collections::BTreeMap, path::Path};
use worldrunner_asset_id::AssetVariant;
use worldrunner_package::AssetInput;

/// Owned read bundle. No native objects, simulation state or filesystem-backed buffers.
pub struct ImportedContent {
    graph: ImportedScene,
    sources: BTreeMap<String, Vec<u8>>,
    profile: ImportProfile,
    sidecar: ImportSidecar,
}
impl ImportedContent {
    pub fn graph(&self) -> &ImportedScene {
        &self.graph
    }
    pub fn sources(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.sources
    }
    /// Reimport against current inputs before export/adoption. Future imports remain valid.
    pub fn verify_current(&self, root: &Path) -> Result<(), ImportError> {
        let current = import_content(
            root,
            Path::new(&self.graph.scene),
            &self.profile,
            &self.sidecar,
        )?;
        if self.graph.scene_guid != current.graph.scene_guid || self.sources != current.sources {
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
pub fn import_content(
    root: &Path,
    scene: &Path,
    profile: &ImportProfile,
    sidecar: &ImportSidecar,
) -> Result<ImportedContent, ImportError> {
    import_content_with_source(root, scene, profile, sidecar, &crate::FileContentSource)
}

pub fn import_content_with_source(
    root: &Path,
    scene: &Path,
    profile: &ImportProfile,
    sidecar: &ImportSidecar,
    source: &dyn crate::ContentSource,
) -> Result<ImportedContent, ImportError> {
    let (graph, sources) = crate::import_inner(root, scene, profile, sidecar, Some(source))?;
    Ok(ImportedContent {
        graph,
        sources,
        profile: profile.clone(),
        sidecar: sidecar.clone(),
    })
}
