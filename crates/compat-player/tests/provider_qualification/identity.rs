use std::path::Path;
use worldrunner::scene::gpu::device::GpuContext;

pub fn altered_tuple_is_rejected(root: &Path, context: &GpuContext) {
    let seal: serde_json::Value =
        serde_json::from_str(include_str!("../../src/providers/qualified-build.json")).unwrap();
    let copy = tempfile::tempdir_in(root.join(".work/resumed-provider-qualification-v2")).unwrap();
    for relative in seal["files"].as_object().unwrap().keys() {
        let destination = copy.path().join(relative);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        let bytes = std::fs::read(root.join(relative)).unwrap();
        std::fs::write(destination, bytes).unwrap();
    }
    for relative in [
        ".work/resumed-provider-qualification-v2/provider.so",
        ".work/resumed-provider-qualification-v2/managed/artifacts/bin/CompatibilityBridge/debug/CompatibilityBridge.dll",
        ".work/resumed-provider-qualification-v2/managed/artifacts/bin/CompatibilityBridge/debug/UnityCompat.pdb",
        ".work/resumed-provider-qualification-v2/api-closure.json",
        ".work/resumed-provider-qualification-v2/api-closure-recovered.json",
        ".work/resumed-provider-qualification-v2/trusted-boundary-inspection.json",
        ".work/resumed-provider-qualification-v2/managed/artifacts/bin/Breakout/debug/Breakout.dll",
        ".work/resumed-provider-qualification-v2/managed/artifacts/bin/Breakout/debug/Breakout.pdb",
        ".work/games/pong/ProjectSettings/InputManager.asset",
        ".work/resumed-provider-qualification-v2/source/Cargo.lock",
        ".work/resumed-provider-qualification-v2/source/crates/compat-player/src/extraction.rs",
        ".work/resumed-provider-qualification-v2/source/crates/compat-player/src/tiles.rs",
        ".work/resumed-provider-qualification-v2/source/crates/compat-player/src/resources.rs",
        ".work/resumed-provider-qualification-v2/source/crates/render-scene/src/dynamic/gpu.rs",
        ".work/resumed-provider-qualification-v2/source/crates/disunity-runtime/src/compatibility.rs",
        ".work/resumed-provider-qualification-v2/libnethost.a",
        "crates/compat-player/src/providers/KeyboardProbe.cs",
        ".work/games/breakout/Assets/Scripts/Platform.cs",
    ] {
        let path = copy.path().join(relative);
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"changed qualification input").unwrap();
        let Err(error) = super::providers::qualify(copy.path(), context) else {
            panic!("altered {relative} accepted");
        };
        assert!(
            error.contains("qualified input changed"),
            "{relative}: {error}"
        );
        std::fs::write(path, original).unwrap();
    }
    let missing = copy
        .path()
        .join(".work/games/breakout/Assets/Scripts/Platform.cs");
    std::fs::remove_file(missing).unwrap();
    assert!(super::providers::qualify(copy.path(), context).is_err());
}

pub fn change_after_qualification_is_rejected(
    root: &Path,
    qualified: &super::providers::QualifiedProviders,
) {
    let path = root.join(".work/resumed-provider-qualification-v2/atlas.png");
    let original = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"different atlas").unwrap();
    let mut inventory = worldrunner::compat_content::TrustedProviderInventory::new();
    let result = qualified.register(&mut inventory);
    std::fs::write(&path, original).unwrap();
    assert!(result.is_err());
    assert!(inventory.bindings().is_empty());
}
