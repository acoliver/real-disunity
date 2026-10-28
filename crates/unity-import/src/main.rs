//! `unity-import`: inspect a Unity project the way the importer reads it.

use std::{collections::BTreeMap, error::Error, path::Path, process::ExitCode};

use worldrunner_unity_import::{
    Guid, ObjectId, Origin, PackageAsset, PackageScript, Project, Reference, Scene, Textures,
    import_scene, input_axes, plan_scene,
    player_objects::{
        OpenFile, decode_census, describe_object, describe_shader_programs, shader_chunks,
        shader_modules,
    },
    player_scene::{SceneNode, read_scene_hierarchy},
    read_player,
    shader_prep::Backend,
};

const USAGE: &str = "usage: unity-import catalog <project>
       unity-import axes <project>
       unity-import plan <project> <scene path>
       unity-import documents <project> <asset path>
       unity-import scene <project> <scene path>
       unity-import texture <project> <texture path>
       unity-import sprites <project> <scene path>
       unity-import components <project> <scene path>
       unity-import player <player data directory>
       unity-import player-decode <player data directory>
       unity-import player-shader-report <player data directory>
       unity-import player-scripts <player data directory> <script layouts JSON>
       unity-import player-scene <player data directory> <scene file>
       unity-import player-object <serialized file> <path ID>
       unity-import player-shader <serialized file> <path ID> <platform index> <output directory>
       unity-import player-shader-programs <serialized file> <path ID> <platform index>
       unity-import player-shader-modules <serialized file> <path ID> <platform index> <output directory>";

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

/// The commands that read a built player, or `None` for any other command.
fn run_player(args: &[String]) -> Option<Result<(), Box<dyn Error>>> {
    Some(match args {
        [command, data] if command == "player" => player(Path::new(data)),
        [command, data] if command == "player-decode" => player_decode(Path::new(data)),
        [command, data, layouts] if command == "player-scripts" => {
            player_scripts(Path::new(data), Path::new(layouts))
        }
        [command, data] if command == "player-shader-report" => {
            player_shader_report(Path::new(data))
        }
        [command, file, path_id] if command == "player-object" => {
            player_object(Path::new(file), path_id)
        }
        [command, file, path_id, platform, out] if command == "player-shader" => {
            player_shader(Path::new(file), path_id, platform, Path::new(out))
        }
        [command, file, path_id, platform, out] if command == "player-shader-modules" => {
            player_shader_modules(Path::new(file), path_id, platform, Path::new(out))
        }
        [command, file, path_id, platform] if command == "player-shader-programs" => {
            player_shader_programs(Path::new(file), path_id, platform)
        }
        [command, data, scene] if command == "player-scene" => {
            read_scene_hierarchy(Path::new(data), scene)
                .map(|roots| roots.iter().for_each(|root| print_node(root, 0)))
                .map_err(Into::into)
        }
        _ => return None,
    })
}

/// Writes each decoded Vulkan stage module to `<blob index>.<slot>.spv` in a new directory.
fn player_shader_modules(
    file: &Path,
    path_id: &str,
    platform: &str,
    out: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut open = OpenFile::open(file)?;
    let modules = shader_modules(&mut open, path_id.parse()?, platform.parse()?)?;
    std::fs::create_dir(out)
        .map_err(|error| format!("cannot create {}: {error}", out.display()))?;
    for module in &modules {
        let name = format!("{}.{}.spv", module.blob_index, module.slot);
        let bytes: Vec<u8> = module.words.iter().flat_map(|w| w.to_le_bytes()).collect();
        std::fs::write(out.join(&name), bytes)?;
        println!(
            "{name} words={} entry_points={:?}",
            module.words.len(),
            module.entry_points
        );
    }
    Ok(())
}

fn player_shader_programs(
    file: &Path,
    path_id: &str,
    platform: &str,
) -> Result<(), Box<dyn Error>> {
    let mut open = OpenFile::open(file)?;
    let text = describe_shader_programs(&mut open, path_id.parse()?, platform.parse()?)?;
    // A platform with no variants prints nothing.
    if !text.is_empty() {
        println!("{text}");
    }
    Ok(())
}

fn player_object(file: &Path, path_id: &str) -> Result<(), Box<dyn Error>> {
    let mut open = OpenFile::open(file)?;
    println!("{}", describe_object(&mut open, path_id.parse()?)?);
    Ok(())
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if let Some(result) = run_player(args) {
        return result;
    }
    match args {
        [command, project] if command == "catalog" => catalog(Path::new(project)),
        [command, project] if command == "axes" => axes(Path::new(project)),
        [command, project, scene] if command == "plan" => {
            plan(Path::new(project), Path::new(scene))
        }
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

/// Writes each decompressed program chunk of one shader platform to `chunk<i>.bin` in a new
/// directory, so no file from an earlier run is left among them.
fn player_shader(
    file: &Path,
    path_id: &str,
    platform: &str,
    out: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut open = OpenFile::open(file)?;
    let (kind, chunks) = shader_chunks(&mut open, path_id.parse()?, platform.parse()?)?;
    std::fs::create_dir(out)
        .map_err(|error| format!("cannot create {}: {error}", out.display()))?;
    for (index, chunk) in chunks.iter().enumerate() {
        std::fs::write(out.join(format!("chunk{index}.bin")), chunk)?;
    }
    let sizes: Vec<usize> = chunks.iter().map(Vec::len).collect();
    println!(
        "platform {kind}: {} chunks of {sizes:?} bytes",
        chunks.len()
    );
    Ok(())
}

fn print_node(node: &SceneNode, depth: usize) {
    let inactive = if node.active { "" } else { " (inactive)" };
    let [x, y, z] = node.local_position;
    println!(
        "{:indent$}{}{inactive} at ({x}, {y}, {z}): {}",
        "",
        node.name,
        node.components.join(", "),
        indent = depth * 2
    );
    for child in &node.children {
        print_node(child, depth + 1);
    }
}

/// Decodes the script fields of every `MonoBehaviour` in a player with script layouts, and
/// prints the counts and the failures by script class. Fails when any script fails.
fn player_scripts(data: &Path, layouts: &Path) -> Result<(), Box<dyn Error>> {
    let json = std::fs::read_to_string(layouts)
        .map_err(|error| format!("cannot read {}: {error}", layouts.display()))?;
    let layouts = worldrunner_unity_import::script_layouts::parse_script_layouts(&json)?;
    let names: Vec<_> = read_player(data)?
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    let census = worldrunner_unity_import::player_scripts::script_census(data, &names, &layouts)?;
    println!(
        "{} of {} script components decode ({} without a script)",
        census.decoded, census.objects, census.missing_scripts
    );
    for (group, (count, examples)) in &census.failures {
        println!("{count} {group}");
        for example in examples {
            println!("    {example}");
        }
    }
    if census.decoded != census.objects {
        return Err(format!(
            "{} script components do not decode",
            census.objects - census.decoded
        )
        .into());
    }
    Ok(())
}

/// Prepares every distinct Vulkan shader module of a player for naga and prints the counts,
/// with the failures by cause and the first module of each.
fn player_shader_report(data: &Path) -> Result<(), Box<dyn Error>> {
    let paths: Vec<_> = read_player(data)?
        .into_iter()
        .map(|entry| data.join(entry.name))
        .collect();
    let report = worldrunner_unity_import::player_shaders::shader_report(&paths)?;
    println!(
        "{} shaders, {} distinct modules, {} ready for naga ({} with split samplers)",
        report.shaders, report.modules, report.ready, report.split
    );
    let written: Vec<String> = Backend::ALL
        .into_iter()
        .map(|backend| {
            let count = report.written.get(&backend).copied().unwrap_or(0);
            format!("{} {count}", backend.name())
        })
        .collect();
    println!("written by naga: {}", written.join(", "));
    for (cause, count) in &report.failures {
        println!("{count} {cause}: {}", report.examples[cause]);
    }
    Ok(())
}

/// Decodes every object whose class has a layout in every serialized file of a player, and
/// prints the counts and the first failures. Fails when any object fails to decode.
fn player_decode(data: &Path) -> Result<(), Box<dyn Error>> {
    let paths: Vec<_> = read_player(data)?
        .into_iter()
        .map(|entry| data.join(entry.name))
        .collect();
    let census = decode_census(&paths)?;
    let mut failed = 0;
    for (class, entry) in &census {
        println!(
            "{class}: {} decoded, {} failed",
            entry.decoded, entry.failed
        );
        for example in &entry.examples {
            println!("  {example}");
        }
        failed += entry.failed;
    }
    if failed > 0 {
        return Err(format!("{failed} objects failed to decode").into());
    }
    Ok(())
}

/// Prints each serialized file of a built player with its objects counted by class, then
/// the totals.
fn player(data: &Path) -> Result<(), Box<dyn Error>> {
    let files = read_player(data)?;
    let mut totals = BTreeMap::<String, usize>::new();
    for entry in &files {
        let file = &entry.file;
        let trees = if file.type_trees {
            "type trees"
        } else {
            "no type trees"
        };
        println!(
            "{}: Unity {}, format {}, {} objects, {} externals, {trees}",
            entry.name,
            file.unity_version,
            file.header.version,
            file.objects().len(),
            file.externals.len(),
        );
        let mut classes = BTreeMap::<String, usize>::new();
        for object in file.objects() {
            *classes.entry(object.class_id.to_string()).or_default() += 1;
        }
        for (class, count) in &classes {
            *totals.entry(class.clone()).or_default() += count;
        }
        print_counts(classes);
    }
    let objects: usize = totals.values().sum();
    println!("total: {} files, {objects} objects", files.len());
    print_counts(totals);
    Ok(())
}

/// Prints counts largest first, ties by name.
fn print_counts(counts: BTreeMap<String, usize>) {
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for (name, count) in counts {
        println!("  {name}: {count}");
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

/// Prints the scene's plan as JSON.
fn plan(root: &Path, path: &Path) -> Result<(), Box<dyn Error>> {
    let plan = plan_scene(&Project::open(root)?, path)?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
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
