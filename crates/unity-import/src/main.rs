//! `unity-import`: inspect a Unity project the way the importer reads it.

use std::{collections::BTreeMap, error::Error, path::Path, process::ExitCode};

use worldrunner_unity_import::{Guid, ObjectId, Origin, Project, Scene, import_scene};

const USAGE: &str = "usage: unity-import catalog <project>
       unity-import documents <project> <asset path>
       unity-import scene <project> <scene path>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("unity-import: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    match args {
        [command, project] if command == "catalog" => catalog(Path::new(project)),
        [command, project, asset] if command == "documents" => {
            documents(Path::new(project), Path::new(asset))
        }
        [command, project, path] if command == "scene" => {
            scene(Path::new(project), Path::new(path))
        }
        _ => Err(USAGE.into()),
    }
}

fn catalog(root: &Path) -> Result<(), Box<dyn Error>> {
    let project = Project::open(root)?;
    let mut importers = BTreeMap::<&str, usize>::new();
    for (_, asset) in project.assets() {
        *importers.entry(asset.importer.as_str()).or_default() += 1;
    }
    println!("editor version: {}", project.editor_version());
    println!("assets: {}", project.assets().count());
    println!("orphan metas: {}", project.orphan_metas().len());
    for (importer, count) in importers {
        let name = if importer.is_empty() {
            "(no importer section)"
        } else {
            importer
        };
        println!("  {name}: {count}");
    }
    Ok(())
}

fn documents(root: &Path, asset: &Path) -> Result<(), Box<dyn Error>> {
    let project = Project::open(root)?;
    let documents = project.read_documents(asset)?;
    let mut kinds = BTreeMap::<(i32, &str), (usize, usize)>::new();
    for document in &documents {
        let entry = kinds
            .entry((document.class_id, document.type_name.as_str()))
            .or_default();
        entry.0 += 1;
        entry.1 += usize::from(document.stripped);
    }
    println!("documents: {}", documents.len());
    for ((class_id, name), (count, stripped)) in kinds {
        println!("  {class_id:>6} {name}: {count} ({stripped} stripped)");
    }
    Ok(())
}

fn scene(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    let project = Project::open(root)?;
    let scene = import_scene(&project, path)?;
    for root in scene.roots() {
        print_tree(&scene, *root, 0);
    }
    let mut origins = BTreeMap::<String, usize>::new();
    for (_, object) in scene.objects() {
        let origin = match object.origin {
            Origin::Scene => "scene".to_owned(),
            Origin::Template(guid) => format!("template {}", asset_name(&project, guid)),
            Origin::Asset(guid) => format!("asset {}", asset_name(&project, guid)),
        };
        *origins.entry(origin).or_default() += 1;
    }
    for (origin, count) in origins {
        println!("{origin}: {count} objects");
    }
    let missing = scene.missing_references();
    println!("missing references: {}", missing.len());
    for (id, reference) in missing {
        let object = scene.object(id).map_or("?", |o| o.type_name.as_str());
        println!("  {object} {}: {reference:?}", id.index());
    }
    Ok(())
}

fn asset_name(project: &Project, guid: Guid) -> String {
    project.asset(guid).map_or_else(
        || guid.to_string(),
        |asset| asset.path.display().to_string(),
    )
}

fn print_tree(scene: &Scene, transform: ObjectId, depth: usize) {
    let Some(view) = scene.transform(transform) else {
        return;
    };
    let object = view.game_object.and_then(|id| scene.game_object(id));
    let name = object.as_ref().map_or("?", |o| o.name.as_str());
    let components: Vec<_> = object
        .iter()
        .flat_map(|o| &o.components)
        .filter_map(|id| scene.object(*id))
        .map(|o| o.type_name.as_str())
        .collect();
    let [x, y, z] = view.local_position;
    println!(
        "{:indent$}{name} [{}] at ({x}, {y}, {z})",
        "",
        components.join(", "),
        indent = depth * 2
    );
    for child in view.children {
        print_tree(scene, child, depth + 1);
    }
}
