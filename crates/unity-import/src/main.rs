//! `unity-import`: inspect a Unity project the way the importer reads it.

use std::{collections::BTreeMap, error::Error, path::Path, process::ExitCode};

use worldrunner_unity_import::Project;

const USAGE: &str =
    "usage: unity-import catalog <project>\n       unity-import documents <project> <asset path>";

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
