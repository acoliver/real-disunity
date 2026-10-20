use super::*;
use crate::scratch::Directory;

fn workspace() -> Directory {
    let dir = Directory::new("layers-test").unwrap();
    dir.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    dir
}

fn package(dir: &Directory, folder: &str, name: &str, extra: &str) {
    dir.write(
        &format!("{folder}/Cargo.toml"),
        &format!("[package]\nname = {name:?}\nversion = \"0.1.0\"\nedition = \"2024\"\n{extra}"),
    )
    .unwrap();
    dir.write(&format!("{folder}/src/lib.rs"), "pub fn value() {}\n")
        .unwrap();
}

fn graph(dir: &Directory) -> Metadata {
    metadata::read(dir.path(), &["--offline", "--all-features"]).unwrap()
}

#[test]
fn concrete_cpu_packages_pass_without_name_heuristics() {
    let dir = workspace();
    package(&dir, "crates/asset", "worldrunner-asset-id", "");
    package(
        &dir,
        "crates/world",
        "worldrunner-world-model",
        "[dependencies]\nrenamed = { package = \"worldrunner-asset-id\", path = \"../asset\" }",
    );
    check(&graph(&dir)).unwrap();
}

#[test]
fn target_optional_renamed_and_build_layer_edges_are_rejected() {
    for section in [
        "dependencies",
        "build-dependencies",
        "dev-dependencies",
        "target.'cfg(windows)'.dependencies",
    ] {
        let dir = workspace();
        package(&dir, "crates/gfx", "worldrunner-render-scene", "");
        let optional = if section.ends_with(".dependencies") || section == "dependencies" {
            ", optional = true"
        } else {
            ""
        };
        package(
            &dir,
            "crates/world",
            "worldrunner-world-model",
            &format!(
                "[{section}]\nrenamed = {{ package = \"worldrunner-render-scene\", path = \"../gfx\"{optional} }}"
            ),
        );
        let error = check(&graph(&dir)).unwrap_err();
        assert!(error.contains("layer violation"), "{error}");
    }
}

#[test]
fn transitive_gpu_and_clr_dependencies_are_rejected_with_chain() {
    for forbidden in ["wgpu", "netcorehost"] {
        let dir = workspace();
        package(
            &dir,
            "crates/asset",
            "worldrunner-asset-id",
            "[dependencies]\nhelper = { path = \"../../vendor/helper\" }",
        );
        package(
            &dir,
            "vendor/helper",
            "helper",
            &format!(
                "[dependencies]\nbackend = {{ package = {forbidden:?}, path = \"../backend\" }}"
            ),
        );
        package(&dir, "vendor/backend", forbidden, "");
        dir.write("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"vendor/helper\", \"vendor/backend\"]\nresolver = \"2\"\n").unwrap();
        let error = check(&graph(&dir)).unwrap_err();
        assert!(
            error.contains(&format!("worldrunner-asset-id -> helper -> {forbidden}")),
            "{error}"
        );
    }
}

#[test]
fn local_paths_cannot_escape_even_through_nonmember_dependencies() {
    let parent = Directory::new("outside-test").unwrap();
    package(&parent, "outside", "helper", "[workspace]");
    parent
        .write(
            "inside/Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
    package(
        &parent,
        "inside/crates/asset",
        "worldrunner-asset-id",
        "[dependencies]\nhelper = { path = \"../../../outside\" }",
    );
    let data = metadata::read(&parent.path().join("inside"), &["--offline"]).unwrap();
    assert!(check(&data).unwrap_err().contains("escapes workspace"));
}

#[test]
fn engine_cannot_depend_on_tool_and_unknown_members_fail_closed() {
    let dir = workspace();
    package(&dir, "crates/tool", "worldrunner-content-tool", "");
    package(
        &dir,
        "crates/asset",
        "worldrunner-asset-id",
        "[dependencies]\nworldrunner-content-tool = { path = \"../tool\" }",
    );
    assert!(check(&graph(&dir)).unwrap_err().contains("layer violation"));
    package(&dir, "crates/unknown", "unclassified", "");
    assert!(
        check(&graph(&dir))
            .unwrap_err()
            .contains("unclassified workspace package")
    );
}

#[test]
fn facade_probe_distinguishes_optional_gpu_from_unconditional_gpu() {
    let dir = workspace();
    package(&dir, "crates/gfx", "worldrunner-render-scene", "");
    let dependencies = "[dependencies]\nworldrunner-render-scene = { path = \"../gfx\", optional = true }\n[features]\ndefault = [\"gpu\"]\ngpu = [\"dep:worldrunner-render-scene\"]\n";
    package(&dir, "crates/worldrunner", "worldrunner", dependencies);
    check_facade(dir.path()).unwrap();
    package(
        &dir,
        "crates/worldrunner",
        "worldrunner",
        "[dependencies]\nworldrunner-render-scene = { path = \"../gfx\" }",
    );
    let error = check_facade(dir.path()).unwrap_err();
    assert!(
        error.contains("worldrunner -> worldrunner-render-scene"),
        "{error}"
    );
}

#[test]
fn game_only_consumes_facade_and_engine_never_consumes_game() {
    let dir = workspace();
    dir.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\", \"games/*\"]\n",
    )
    .unwrap();
    package(&dir, "crates/asset", "worldrunner-asset-id", "");
    package(
        &dir,
        "games/example",
        "example",
        "[dependencies]\nworldrunner-asset-id = { path = \"../../crates/asset\" }",
    );
    assert!(check(&graph(&dir)).unwrap_err().contains("layer violation"));
    package(&dir, "games/example", "example", "");
    package(
        &dir,
        "crates/asset",
        "worldrunner-asset-id",
        "[dependencies]\nexample = { path = \"../../games/example\" }",
    );
    assert!(check(&graph(&dir)).unwrap_err().contains("layer violation"));
}

#[test]
fn ci_profiles_select_disjoint_managed_and_gpu_requirements() {
    use crate::commands::{Profile, selection};
    let dir = workspace();
    for (folder, name) in [
        ("asset", "worldrunner-asset-id"),
        ("ffi", "worldrunner-disunity-ffi"),
        ("gfx", "worldrunner-render-scene"),
        ("shadow", "worldrunner-render-shadow"),
        ("host", "worldrunner-scene-viewer"),
        ("managed", "worldrunner-disunity-runtime"),
        ("tool", "xtask"),
        ("worldrunner", "worldrunner"),
    ] {
        package(&dir, &format!("crates/{folder}"), name, "");
    }
    let data = graph(&dir);
    let cpu = selection(&data, Profile::Cpu).unwrap();
    assert!(cpu.contains(&"worldrunner-asset-id".into()));
    assert!(cpu.contains(&"xtask".into()));
    for absent in [
        "worldrunner-render-scene",
        "worldrunner-render-shadow",
        "worldrunner-scene-viewer",
        "worldrunner-disunity-runtime",
        "--workspace",
    ] {
        assert!(!cpu.contains(&absent.into()), "{cpu:?}");
    }
    let gpu = selection(&data, Profile::Gpu).unwrap();
    assert!(gpu.contains(&"worldrunner-render-scene".into()));
    assert!(gpu.contains(&"worldrunner-render-shadow".into()));
    assert!(gpu.contains(&"worldrunner-scene-viewer".into()));
    assert!(gpu.contains(&"worldrunner".into()));
    assert!(!gpu.contains(&"worldrunner-disunity-runtime".into()));
    let managed = selection(&data, Profile::Managed).unwrap();
    assert!(managed.contains(&"worldrunner-disunity-runtime".into()));
    assert!(!managed.contains(&"worldrunner-render-scene".into()));
    assert!(!managed.contains(&"worldrunner-render-shadow".into()));
}

#[test]
fn shared_shadow_backend_is_graphics_and_rejected_from_cpu_graphs() {
    let dir = workspace();
    package(&dir, "crates/shadow", "worldrunner-render-shadow", "");
    package(
        &dir,
        "crates/scene",
        "worldrunner-render-scene",
        "[dependencies]\nworldrunner-render-shadow = { path = \"../shadow\" }",
    );
    let data = graph(&dir);
    assert_eq!(
        workspace_layers(&data).unwrap()["worldrunner-render-shadow"],
        Layer::Graphics
    );
    check(&data).unwrap();
    assert!(forbidden("worldrunner-render-shadow"));

    package(
        &dir,
        "crates/asset",
        "worldrunner-asset-id",
        "[dependencies]\nhelper = { path = \"../../vendor/helper\" }",
    );
    package(
        &dir,
        "vendor/helper",
        "helper",
        "[dependencies]\nrenamed = { package = \"worldrunner-render-shadow\", path = \"../../crates/shadow\" }",
    );
    dir.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"vendor/helper\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    let error = check(&graph(&dir)).unwrap_err();
    assert!(
        error.contains("worldrunner-asset-id -> helper -> worldrunner-render-shadow"),
        "{error}"
    );
}

#[test]
fn excluded_game_paths_are_not_an_engine_dependency_escape_hatch() {
    let dir = workspace();
    dir.write(
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"games/example\"]\n",
    )
    .unwrap();
    package(&dir, "games/example", "example", "");
    package(
        &dir,
        "crates/asset",
        "worldrunner-asset-id",
        "[dependencies]\nexample = { path = \"../../games/example\" }",
    );
    let error = check(&graph(&dir)).unwrap_err();
    assert!(error.contains("games/tools path"), "{error}");
}
