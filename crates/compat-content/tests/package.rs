use std::{collections::BTreeMap, path::PathBuf};
use worldrunner_asset_id::{AssetVariant, LogicalAssetId, QualityTier};
use worldrunner_catalog::{CatalogBuilder, ResolutionPolicy};
use worldrunner_compat_content::{ImportProfile, ImportSidecar, import_project};
use worldrunner_package::{Package, PackageBuilder, PackageKind, PackageLimits, PackageManifest};

#[test]
fn canonical_package_and_catalog_resolve_imported_graph_and_actual_source_assets() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.work/games/snake");
    let g = import_project(
        &root,
        "Assets/Scenes/GameScene.unity".as_ref(),
        &ImportProfile::default(),
        &ImportSidecar::default(),
    )
    .unwrap();
    let sources: BTreeMap<_, _> = g
        .source_hashes
        .keys()
        .map(|path| (path.clone(), std::fs::read(root.join(path)).unwrap()))
        .collect();
    let variant = AssetVariant {
        art_set: "original".into(),
        quality: QualityTier::Baseline,
        platform_profile: "portable".into(),
    };
    let inputs = g
        .package_inputs(sources.clone(), &variant, "original-selected-scene")
        .unwrap();
    assert_eq!(inputs.len(), g.assets.len() + 1);
    let manifest = PackageManifest {
        package_id: "imported".into(),
        semantic_version: "1.0.0".into(),
        package_kind: PackageKind::Development,
        content_schema: 1,
    };
    let mut builder = PackageBuilder::new(manifest);
    for input in inputs {
        builder.add(input);
    }
    let built = builder.build().unwrap();
    let reader = Package::parse(built.bytes, PackageLimits::default()).unwrap();
    let mut catalog = CatalogBuilder::new();
    catalog
        .mount(reader.manifest(), reader.entries(), 0)
        .unwrap();
    let id = LogicalAssetId::new(format!("unity.{}.graph", g.scene_guid)).unwrap();
    let order = catalog
        .build()
        .resolve_dependency_order(
            &id,
            &ResolutionPolicy {
                art_sets: vec!["original".into()],
                platform_profiles: vec!["portable".into()],
                maximum_quality: QualityTier::Baseline,
            },
        )
        .unwrap();
    assert_eq!(order.len(), g.assets.len() + 1);
    let mut stale = sources;
    stale
        .get_mut("Assets/GameConfigs/GameConfig.asset")
        .unwrap()
        .push(b'\n');
    assert!(
        g.package_inputs(stale, &variant, "original-selected-scene")
            .is_err()
    );
}
