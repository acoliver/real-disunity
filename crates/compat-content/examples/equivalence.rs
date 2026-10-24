use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
use worldrunner_compat_content::{
    AdmissionPolicy, ContentReceipt, ImportProfile, ImportSidecar, ImportedScene, import_project,
    import_receipt, materialization_plan, script_census,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let scenes = [
        ("snake", "Assets/Scenes/GameScene.unity"),
        ("breakout", "Assets/Scenes/SampleScene.unity"),
        ("pong", "Assets/Scenes/Main.unity"),
    ]
    .into_iter()
    .map(|(name, scene)| verify_original(name, scene))
    .collect::<Result<Vec<_>>>()?;
    serde_json::to_writer_pretty(std::io::stdout().lock(), &scenes)?;
    Ok(())
}
fn verify_original(name: &str, scene: &str) -> Result<Value> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = base.join("../..");
    let source = root.join(".work/games").join(name);
    let profile: ImportProfile = serde_json::from_slice(&fs::read(
        base.join(format!("profiles/{name}-source.json")),
    )?)?;
    let sidecar: ImportSidecar =
        serde_json::from_slice(&fs::read(base.join(format!("overlays/{name}.json")))?)?;
    let policy: AdmissionPolicy = serde_json::from_slice(&fs::read(
        base.join(format!("profiles/{name}-admission.json")),
    )?)?;
    let graph = import_project(&source, scene.as_ref(), &profile, &sidecar)?;
    let receipt = import_receipt(&source, scene.as_ref(), &profile, &sidecar)?;
    compare_receipt(&source, &graph, &receipt)?;
    receipt.verify_current(&source)?;
    let scripts = script_census(&source, "Assets/Scripts".as_ref(), &profile)?;
    compare_baseline(&root, name, &graph, &scripts)?;
    let grid = graph
        .components
        .iter()
        .filter_map(|c| c.script.as_ref())
        .find(|s| s.type_name == "BrickSpawner");
    let bricks = grid.map(|s| {
        s.fields["columns"].as_u64().unwrap()
            * s.fields["brickTypes"].as_array().unwrap().len() as u64
    });
    let walls = graph
        .components
        .iter()
        .find(|c| c.kind == "TilemapCollider2D")
        .and_then(|c| c.native_fields["cells"].as_array())
        .map(Vec::len);
    let admission_error = materialization_plan(receipt.graph(), &policy)
        .err()
        .map(|e| e.to_string());
    let missing: Vec<_> = receipt
        .graph()
        .diagnostics
        .iter()
        .filter(|d| {
            d.severity == "error"
                && (d.path.contains(".meta") || d.path.starts_with("ProjectSettings/"))
        })
        .collect();
    Ok(
        json!({"scene":name,"nodes":graph.nodes.len(),"components":graph.components.len(),
        "brick_grid_capacity":bricks,"authored_wall_cells":walls,"assets":graph.assets.len(),"scripts":scripts.len(),"receipt_files":receipt.sources().len(),
        "closure_sha256":receipt.closure_sha256(),"retained_graph_fields_equal":true,
        "receipt_component_node_fields_equal":true,"source_bytes_equal":true,
        "strict_receipt_admission_error":admission_error,"metadata_errors":missing}),
    )
}
fn compare_receipt(source: &Path, graph: &ImportedScene, receipt: &ContentReceipt) -> Result<()> {
    for component in &graph.components {
        let current = receipt
            .graph()
            .components
            .iter()
            .find(|c| c.id == component.id)
            .ok_or("component removed")?;
        assert_eq!(
            serde_json::to_value(component)?,
            serde_json::to_value(current)?
        );
    }
    for node in &graph.nodes {
        let current = receipt
            .graph()
            .nodes
            .iter()
            .find(|n| n.id == node.id)
            .ok_or("node removed")?;
        assert_eq!(serde_json::to_value(node)?, serde_json::to_value(current)?);
    }
    for (path, hash) in &graph.source_hashes {
        assert_eq!(receipt.graph().source_hashes.get(path), Some(hash));
        assert_eq!(
            receipt.sources().get(path),
            Some(&fs::read(source.join(path))?)
        );
    }
    Ok(())
}

fn compare_baseline(
    root: &Path,
    name: &str,
    graph: &ImportedScene,
    scripts: &[worldrunner_compat_content::ScriptSource],
) -> Result<()> {
    let old_census: Value = serde_json::from_slice(&fs::read(
        root.join(format!(".work/content-next/all/{name}-census.json")),
    )?)?;
    assert_eq!(
        serde_json::to_value(scripts)?,
        old_census,
        "{name} runtime sources changed"
    );
    let baseline: Value = serde_json::from_slice(&fs::read(
        root.join(format!(".work/content-next/all/{name}-ready.json")),
    )?)?;
    for field in [
        "nodes",
        "components",
        "documents",
        "assets",
        "source_hashes",
        "aliases",
    ] {
        assert_eq!(
            serde_json::to_value(graph)?[field],
            baseline[field],
            "{name} {field} changed"
        );
    }
    Ok(())
}
