//! Runs a built Unity player's scene headless in the managed host.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use worldrunner_disunity_runtime::{DisunityHost, ManagedHostConfig};
use worldrunner_original_games::workspace_root;
use worldrunner_unity_import::{
    player_identity::{PlayerIdentity, player_identity, player_quality, player_scene_paths},
    player_plan::plan_player_scene,
    player_resources::plan_player_resources,
    script_layouts::{ScriptLayouts, parse_script_layouts},
};

use crate::PlayError;
use crate::prepare::{FIRST_TEXTURE, bridge_config, dotnet, run, target, write};

/// A player scene ready to boot: the host configuration and the request its bridge loads.
pub struct PreparedPlayer {
    pub config: ManagedHostConfig,
    pub request: PathBuf,
}

/// Writes the script layouts of the player in `data` (its `<Game>_Data` directory), plans
/// scene file `scene` and the player's resources with them, and builds the managed bridge,
/// all into `<target>/play/player`. `arguments` become the player's command line after its
/// executable path.
pub fn prepare_player(
    data: &Path,
    scene: &str,
    arguments: &[String],
) -> Result<PreparedPlayer, PlayError> {
    let root = workspace_root()?;
    let output = target(&root).join("play").join("player");
    std::fs::create_dir_all(&output).map_err(|error| PlayError::Io(output.clone(), error))?;
    let layouts = script_layouts(&root, &output, data)?;
    let plan = plan_player_scene(data, scene, &layouts)?;
    eprintln!(
        "[player] planned {scene}: {} objects, {} differences",
        plan.objects.len(),
        plan.differences.len()
    );
    let plan_path = output.join("plan.json");
    let plan_bytes = serde_json::to_vec(&plan).expect("plans serialize");
    write(&plan_path, &plan_bytes)?;
    let scenes = plan_build_scenes(data, &layouts, &output, (scene, &plan_bytes))?;
    let resources = plan_player_resources(data, &layouts, &output)?;
    eprintln!(
        "[player] planned resources: {} entries, {} assets",
        resources.entries.len(),
        resources.assets.len()
    );
    let resources_path = output.join("resources.json");
    write(
        &resources_path,
        &serde_json::to_vec(&resources).expect("plans serialize"),
    )?;
    let request = write_request(
        &output,
        data,
        [&plan_path, &resources_path],
        (&scenes, arguments),
    )?;
    Ok(PreparedPlayer {
        config: bridge_config(&root, &output)?,
        request,
    })
}

/// Builds the layouts tool, has it write the player's script layouts and reads them.
fn script_layouts(root: &Path, output: &Path, data: &Path) -> Result<ScriptLayouts, PlayError> {
    let tool = output.join("layouts-tool");
    dotnet(&[
        root.join("csharp/Disunity.Layouts/Disunity.Layouts.csproj")
            .as_os_str(),
        "-o".as_ref(),
        tool.as_os_str(),
    ])?;
    let layouts_path = output.join("layouts.json");
    let summary = run(Command::new(crate::prepare::dotnet_host())
        .arg(tool.join("Disunity.Layouts.dll"))
        .arg(data)
        .arg(&layouts_path))?;
    eprintln!("[player] script layouts: {}", summary.trim());
    let json = std::fs::read_to_string(&layouts_path)
        .map_err(|error| PlayError::Io(layouts_path.clone(), error))?;
    Ok(parse_script_layouts(&json)?)
}

/// The file name a scene path is known by: its last component without `.unity`.
fn scene_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".unity").unwrap_or(file)
}

/// The request's `scenes` list: one entry per build scene in build order, with the plan file
/// for the scenes that planned and `null` for those that did not.
fn scene_entries(paths: &[String], plans: &[Option<PathBuf>]) -> Vec<serde_json::Value> {
    paths
        .iter()
        .zip(plans)
        .map(|(path, plan)| {
            serde_json::json!({"name": scene_name(path), "path": path, "plan": plan})
        })
        .collect()
}

/// Plans every scene in the player's build into `scene-{i}.json` under `output`, reusing
/// `requested`'s bytes for the scene it already planned. A scene that fails to plan is
/// reported on standard error and listed without a plan, so only loading it fails.
fn plan_build_scenes(
    data: &Path,
    layouts: &ScriptLayouts,
    output: &Path,
    requested: (&str, &[u8]),
) -> Result<Vec<serde_json::Value>, PlayError> {
    let paths = player_scene_paths(data)?;
    let mut plans = Vec::with_capacity(paths.len());
    for index in 0..paths.len() {
        let file = format!("level{index}");
        let planned = if file == requested.0 {
            Ok(requested.1.to_vec())
        } else {
            plan_player_scene(data, &file, layouts)
                .map(|plan| serde_json::to_vec(&plan).expect("plans serialize"))
        };
        plans.push(match planned {
            Ok(bytes) => {
                let path = output.join(format!("scene-{index}.json"));
                write(&path, &bytes)?;
                Some(path)
            }
            Err(error) => {
                eprintln!("[player] could not plan {file}: {error}");
                None
            }
        });
    }
    eprintln!("[player] planned {} build scenes", paths.len());
    Ok(scene_entries(&paths, &plans))
}

/// Writes the bridge's request: the player's paths, plans, identity, command line arguments
/// and quality levels. Each game keeps its preferences in its own
/// `persistent/<company>/<product>` directory under `output`. `plans` are the scene plan and
/// the resource plan; `launch` is the build's scene list and the command line.
fn write_request(
    output: &Path,
    data: &Path,
    plans: [&Path; 2],
    launch: (&[serde_json::Value], &[String]),
) -> Result<PathBuf, PlayError> {
    let [plan, resources] = plans;
    let (scenes, arguments) = launch;
    let identity = player_identity(data)?;
    eprintln!(
        "[player] {} {} {} built with Unity {} for {}",
        identity.company_name,
        identity.product_name,
        identity.version,
        identity.unity_version,
        identity.platform
    );
    let persistent = output
        .join("persistent")
        .join(path_component(&identity.company_name))
        .join(path_component(&identity.product_name));
    let request = output.join("request.json");
    let mut body = request_body(data, [plan, resources, &persistent], &identity, arguments);
    body["scenes"] = serde_json::Value::Array(scenes.to_vec());
    if let Some(quality) = player_quality(data)? {
        body["quality"] = serde_json::json!(quality);
    }
    write(&request, body.to_string().as_bytes())?;
    Ok(request)
}

/// The bridge's request. `paths` are the scene plan, the resource plan and the persistent data
/// directory. The managed side derives the player's executable path from `player` and the
/// platform in `identity`, and puts `arguments` after it in `Environment.GetCommandLineArgs()`.
fn request_body(
    data: &Path,
    paths: [&Path; 3],
    identity: &PlayerIdentity,
    arguments: &[String],
) -> serde_json::Value {
    let [plan, resources, persistent] = paths;
    serde_json::json!({
        "player": data,
        "plan": plan,
        "resources": resources,
        "identity": identity,
        "persistent": persistent,
        "arguments": arguments,
        "first_texture": FIRST_TEXTURE,
    })
}

/// A name as one path component: characters that separate or escape paths become `_`, and an
/// empty name becomes `_`.
fn path_component(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '.') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if safe.is_empty() {
        "_".to_owned()
    } else {
        safe
    }
}

/// Boots the host, starts the player's scene and advances it `frames` times by `dt`. The
/// error names the step that failed: starting the scene or the frame that failed.
pub fn run_player(prepared: &PreparedPlayer, frames: u64, dt: Duration) -> Result<(), PlayError> {
    let mut host = DisunityHost::boot(&prepared.config)?;
    host.run_slice(&prepared.request)?;
    let nanos = u64::try_from(dt.as_nanos())
        .map_err(|_| PlayError::Invalid("the frame delta is too long".into()))?;
    let mut result = Ok(());
    for frame in 0..frames {
        if let Err(error) = host.advance(nanos, 1_000_000_000) {
            result = Err(PlayError::Invalid(format!("frame {frame}: {error}")));
            break;
        }
    }
    if result.is_ok() {
        eprintln!("[player] ran {frames} frames");
    }
    // A frame's failure says more than an unload failure that follows it.
    let unloaded = host.unload_slice();
    result.and(unloaded.map_err(PlayError::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> PlayerIdentity {
        PlayerIdentity {
            company_name: "Company".into(),
            product_name: "Product".into(),
            version: "1.0".into(),
            unity_version: "6000.3.15f1".into(),
            platform: "WindowsPlayer",
        }
    }

    fn body(arguments: &[String]) -> serde_json::Value {
        let (plan, resources, persistent) = (
            Path::new("plan.json"),
            Path::new("resources.json"),
            Path::new("persistent"),
        );
        request_body(
            Path::new("/games/Game/Game_Data"),
            [plan, resources, persistent],
            &identity(),
            arguments,
        )
    }

    #[test]
    fn the_request_carries_the_arguments_in_order_one_value_each() {
        let arguments: Vec<String> = ["-userdata", "/tmp/my saves", "-headless", "-userdata", "x"]
            .map(String::from)
            .to_vec();
        let request = body(&arguments);
        assert_eq!(
            request["arguments"],
            serde_json::json!(["-userdata", "/tmp/my saves", "-headless", "-userdata", "x"])
        );
    }

    #[test]
    fn the_scene_list_names_each_build_scene_and_leaves_a_failed_plan_null() {
        let paths: Vec<String> = [
            "Assets/GameAssets/Scenes/StartScreen.unity",
            "Assets/GameAssets/Scenes/MainGame.unity",
            "Assets/Loose",
        ]
        .map(String::from)
        .to_vec();
        let plans = [
            Some(PathBuf::from("out/scene-0.json")),
            None,
            Some(PathBuf::from("out/scene-2.json")),
        ];
        assert_eq!(
            serde_json::Value::Array(scene_entries(&paths, &plans)),
            serde_json::json!([
                {"name": "StartScreen", "path": "Assets/GameAssets/Scenes/StartScreen.unity", "plan": "out/scene-0.json"},
                {"name": "MainGame", "path": "Assets/GameAssets/Scenes/MainGame.unity", "plan": null},
                {"name": "Loose", "path": "Assets/Loose", "plan": "out/scene-2.json"},
            ])
        );
    }

    #[test]
    fn the_request_names_the_data_directory_and_platform_the_executable_path_comes_from() {
        let request = body(&[]);
        assert_eq!(request["arguments"], serde_json::json!([]));
        assert_eq!(request["player"], "/games/Game/Game_Data");
        assert_eq!(request["identity"]["platform"], "WindowsPlayer");
    }
}
