//! `unity-import`: inspect a Unity project the way the importer reads it.

use std::{collections::BTreeMap, error::Error, path::Path, process::ExitCode};

use worldrunner_unity_import::{
    Guid, ObjectId, Origin, PackageAsset, PackageScript, Project, Reference, Scene, Textures,
    import_scene, input_axes,
};

const USAGE: &str = "usage: unity-import catalog <project>
       unity-import axes <project>
       unity-import documents <project> <asset path>
       unity-import scene <project> <scene path>
       unity-import texture <project> <texture path>
       unity-import sprites <project> <scene path>
       unity-import components <project> <scene path>";

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
        [command, project] if command == "axes" => axes(Path::new(project)),
        [command, project, asset] if command == "documents" => {
            documents(Path::new(project), Path::new(asset))
        }
        [command, project, path] if command == "scene" => {
            scene(Path::new(project), Path::new(path))
        }
        [command, project, path] if command == "texture" => {
            texture(Path::new(project), Path::new(path))
        }
        [command, project, path] if command == "sprites" => {
            sprites(Path::new(project), Path::new(path))
        }
        [command, project, path] if command == "components" => {
            components(Path::new(project), Path::new(path))
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

fn texture(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    let project = Project::open(root)?;
    let guid = project
        .guid_of(path)
        .ok_or_else(|| format!("{} is not a cataloged asset", path.display()))?;
    let mut textures = Textures::default();
    let texture = textures.get(&project, guid)?;
    let image = texture.image();
    println!(
        "{} {}x{}",
        texture.path().display(),
        image.width,
        image.height
    );
    println!("{:?}", texture.settings());
    for sprite in texture.sprites() {
        println!(
            "  {} {}: rect {:?} pivot {:?} ppu {}",
            sprite.file_id, sprite.name, sprite.rect, sprite.pivot, sprite.pixels_per_unit
        );
    }
    Ok(())
}

/// Resolves every reference from the scene's objects into a texture asset.
fn sprites(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    let project = Project::open(root)?;
    let scene = import_scene(&project, path)?;
    let mut references = Vec::new();
    for (_, object) in scene.objects() {
        object.fields.visit_references(&mut |reference| {
            if let Reference::External { guid, file_id } = *reference {
                let texture = project
                    .asset(guid)
                    .is_some_and(|asset| asset.importer == "TextureImporter");
                if texture {
                    references.push((guid, file_id));
                }
            }
        });
    }
    let mut textures = Textures::default();
    let mut failures = 0;
    for (guid, file_id) in &references {
        if let Err(error) = textures.sprite(&project, *guid, *file_id) {
            failures += 1;
            println!("  unresolved: {error}");
        }
    }
    for (_, texture) in textures.iter() {
        let image = texture.image();
        println!(
            "{} {}x{}: {} sprites",
            texture.path().display(),
            image.width,
            image.height,
            texture.sprites().len()
        );
    }
    println!(
        "sprite references: {} ({failures} unresolved)",
        references.len()
    );
    Ok(())
}

/// The typed view of a package script component, for the scripts that have one.
fn package_view(scene: &Scene, id: ObjectId) -> Option<String> {
    let Some(Reference::Package(PackageAsset::Script(script))) = scene.script(id) else {
        return None;
    };
    Some(match script {
        PackageScript::Text => format!("{:?}", scene.text(id)),
        PackageScript::CanvasScaler => format!("{:?}", scene.canvas_scaler(id)),
        PackageScript::Light2D => format!("{:?}", scene.light_2d(id)),
        PackageScript::UniversalAdditionalCameraData => format!("{:?}", scene.urp_camera(id)),
        other => format!("{other:?}"),
    })
}

/// The typed view of an engine component, for the classes that have one.
fn engine_view(scene: &Scene, id: ObjectId, type_name: &str) -> Option<String> {
    Some(match type_name {
        "Grid" => format!("{:?}", scene.grid(id)),
        "Tilemap" => scene.tilemap(id).map_or_else(
            |error| error.to_string(),
            |map| format!("{} tiles, first {:?}", map.tiles.len(), map.tiles.first()),
        ),
        "Camera" => format!("{:?}", scene.camera(id)),
        "AudioListener" | "Behaviour" => format!("{:?}", scene.camera_participant(id)),
        "Rigidbody2D" => format!("{:?}", scene.rigidbody_2d(id)),
        name if name.ends_with("Collider2D") => format!(
            "{:?}, material {:?}",
            scene.collider(id),
            scene.collider_material(id)
        ),
        "SpriteRenderer" => format!("{:?}", scene.sprite_renderer(id)),
        "TilemapRenderer" => format!("{:?}", scene.tilemap_renderer(id)),
        "Canvas" => format!("{:?}", scene.canvas(id)),
        "RectTransform" => format!("{:?}", scene.rect_transform(id)),
        "MonoBehaviour" => package_view(scene, id)?,
        _ => return None,
    })
}

/// Reads every typed component view in the scene.
fn components(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    let project = Project::open(root)?;
    let scene = import_scene(&project, path)?;
    for (id, object) in scene.objects() {
        if let Some(line) = engine_view(&scene, id, &object.type_name) {
            println!("{} {}: {line}", object.type_name, id.index());
        }
    }
    Ok(())
}

/// Prints the project's input axes in file order.
fn axes(root: &Path) -> Result<(), Box<dyn Error>> {
    for axis in input_axes(&Project::open(root)?)? {
        println!(
            "{} {:?}: -{:?}/{:?} +{:?}/{:?}",
            axis.name,
            axis.kind,
            axis.negative,
            axis.alt_negative,
            axis.positive,
            axis.alt_positive
        );
    }
    Ok(())
}
