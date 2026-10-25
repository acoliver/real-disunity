use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

fn output(command: &mut Command) -> Value {
    let output = command.output().expect("graph tool installed");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("graph JSON")
}
fn runtime_graph(graph: &Value) -> Result<(), String> {
    let packages: BTreeMap<_, _> = graph["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let root = packages
        .iter()
        .find(|(_, name)| **name == "worldrunner-disunity-runtime")
        .unwrap()
        .0;
    let nodes: BTreeMap<_, _> = graph["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n))
        .collect();
    let mut pending = vec![*root];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        if packages[id] == "worldrunner-disunity-ffi" {
            return Err("runtime statically links the FFI image".into());
        }
        pending.extend(
            nodes[id]["deps"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|edge| {
                    edge["dep_kinds"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|kind| kind["kind"] != "dev")
                })
                .map(|edge| edge["pkg"].as_str().unwrap()),
        );
    }
    Ok(())
}
pub fn assert_runtime_graph(root: &Path) {
    let graph = output(
        Command::new("cargo")
            .args(["metadata", "--locked", "--offline", "--format-version", "1"])
            .current_dir(root),
    );
    runtime_graph(&graph).unwrap();
}
fn managed_graph(graph: &Value, consumer: bool) -> Result<(), String> {
    let items = &graph["Items"];
    if !items["PackageReference"].as_array().unwrap().is_empty() {
        return Err("unexpected managed package".into());
    }
    let projects = items["ProjectReference"].as_array().unwrap();
    if projects.len() != usize::from(consumer)
        || projects.iter().any(|p| p["Filename"] != "Disunity.Engine")
    {
        return Err("unexpected managed project reference".into());
    }
    let references = items["ReferencePath"].as_array().unwrap();
    let custom: Vec<_> = references
        .iter()
        .filter(|r| {
            r["FrameworkReferenceName"]
                .as_str()
                .unwrap_or("")
                .is_empty()
        })
        .collect();
    if custom.len() != usize::from(consumer)
        || custom.iter().any(|r| r["Filename"] != "Disunity.Engine")
    {
        return Err("unexpected resolved assembly reference".into());
    }
    Ok(())
}
pub fn assert_managed_graph(root: &Path) {
    let restore = Command::new("dotnet")
        .arg("restore")
        .arg(root.join("csharp/UnityConsumer/UnityConsumer.csproj"))
        .output()
        .expect("dotnet installed");
    assert!(
        restore.status.success(),
        "managed restore failed:
{}
{}",
        String::from_utf8_lossy(&restore.stdout),
        String::from_utf8_lossy(&restore.stderr)
    );
    for (project, consumer) in [
        ("UnityConsumer/UnityConsumer.csproj", true),
        ("Disunity/Disunity.Engine.csproj", false),
    ] {
        let graph = output(
            Command::new("dotnet")
                .arg("msbuild")
                .arg(root.join("csharp").join(project))
                .args([
                    "-t:ResolveReferences",
                    "-getItem:ProjectReference,ReferencePath,PackageReference",
                    "-v:q",
                    "-nologo",
                ]),
        );
        managed_graph(&graph, consumer).unwrap();
    }
}
#[test]
fn aliased_resolved_dependency_is_rejected() {
    let graph = serde_json::json!({"packages":[{"id":"host","name":"worldrunner-disunity-runtime"},{"id":"image","name":"worldrunner-disunity-ffi"}],"resolve":{"nodes":[{"id":"host","deps":[{"name":"hidden_alias","pkg":"image","dep_kinds":[{"kind":null}]}]},{"id":"image","deps":[]}]}});
    assert!(runtime_graph(&graph).is_err());
}
#[test]
fn imported_and_transitive_managed_references_are_rejected() {
    let mut graph = serde_json::json!({"Items":{"PackageReference":[],"ProjectReference":[],"ReferencePath":[]}});
    graph["Items"]["ProjectReference"] = serde_json::json!([{"Filename":"RequestTransport","DefiningProjectFullPath":"Imported.props"}]);
    assert!(managed_graph(&graph, false).is_err());
    graph["Items"]["ProjectReference"] = serde_json::json!([]);
    graph["Items"]["ReferencePath"] = serde_json::json!([{"Filename":"RequestTransport","ReferenceSourceTarget":"ProjectReference"}]);
    assert!(managed_graph(&graph, false).is_err());
}

fn fixture(name: &str) -> std::path::PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/validation")
        .join(format!("graph-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    root
}
#[test]
fn cargo_metadata_rejects_an_actual_aliased_image() {
    let root = fixture("alias");
    for (name, manifest) in [
        (
            "host",
            "[workspace]\n[package]\nname='worldrunner-disunity-runtime'\nversion='0.0.0'\nedition='2024'\n[dependencies]\nhidden_image={package='worldrunner-disunity-ffi',path='../image'}\n",
        ),
        (
            "image",
            "[package]\nname='worldrunner-disunity-ffi'\nversion='0.0.0'\nedition='2024'\n",
        ),
    ] {
        std::fs::create_dir_all(root.join(name).join("src")).unwrap();
        std::fs::write(root.join(name).join("Cargo.toml"), manifest).unwrap();
        std::fs::write(root.join(name).join("src/lib.rs"), "").unwrap();
    }
    let graph = output(
        Command::new("cargo")
            .args(["metadata", "--offline", "--format-version", "1"])
            .current_dir(root.join("host")),
    );
    assert!(runtime_graph(&graph).is_err());
}
#[test]
fn evaluated_imported_reference_is_rejected() {
    let root = fixture("import");
    std::fs::write(root.join("Consumer.csproj"), "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup><Import Project=\"Injected.props\" /></Project>").unwrap();
    std::fs::write(root.join("Injected.props"), "<Project><ItemGroup><ProjectReference Include=\"RequestTransport.csproj\" /></ItemGroup></Project>").unwrap();
    let graph = output(
        Command::new("dotnet")
            .arg("msbuild")
            .arg(root.join("Consumer.csproj"))
            .args([
                "-getItem:ProjectReference,ReferencePath,PackageReference",
                "-v:q",
                "-nologo",
            ]),
    );
    assert!(managed_graph(&graph, false).is_err());
}
