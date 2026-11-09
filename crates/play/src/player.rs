//! Runs a built Unity player's scene headless in the managed host.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

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
use crate::{driver::FIRST_COVERAGE_TEXTURE, gpu_player::PlayerGpu, player_meshes::PlayerMeshes};

/// A player scene ready to boot: the host configuration and the request its bridge loads.
pub struct PreparedPlayer {
    pub config: ManagedHostConfig,
    pub request: PathBuf,
    /// The player's `<Game>_Data` directory, where its shaders and serialized files are read.
    pub data: PathBuf,
}

/// How a player is started besides its files: the command line after its executable path,
/// the size of the window the frame is drawn into, which is the size the player's `Screen`
/// reports (a headless run gives none), and extra native plugin directories the player
/// searches after its own `Plugins` directories and the engine's substitutes.
#[derive(Clone, Copy, Default)]
pub struct PlayerLaunch<'a> {
    pub arguments: &'a [String],
    pub screen: Option<[u32; 2]>,
    pub plugins: &'a [PathBuf],
}

/// Writes the script layouts of the player in `data` (its `<Game>_Data` directory), plans
/// scene file `scene` and the player's resources with them, and builds the managed bridge,
/// all into `<target>/play/player`. `launch` says how the player starts.
pub fn prepare_player(
    data: &Path,
    scene: &str,
    launch: PlayerLaunch,
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
        (&scenes, launch),
    )?;
    Ok(PreparedPlayer {
        config: bridge_config(&root, &output)?,
        request,
        data: data.to_owned(),
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
/// for the scenes that planned, and `null` plus the planning `error` for those that did not.
fn scene_entries(paths: &[String], plans: &[Result<PathBuf, String>]) -> Vec<serde_json::Value> {
    paths
        .iter()
        .zip(plans)
        .map(|(path, plan)| {
            let name = scene_name(path);
            match plan {
                Ok(plan) => serde_json::json!({"name": name, "path": path, "plan": plan}),
                Err(error) => {
                    serde_json::json!({"name": name, "path": path, "plan": null, "error": error})
                }
            }
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
                Ok(path)
            }
            Err(error) => {
                eprintln!("[player] could not plan {file}: {error}");
                Err(error.to_string())
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
    launch: (&[serde_json::Value], PlayerLaunch),
) -> Result<PathBuf, PlayError> {
    let [plan, resources] = plans;
    let (scenes, launch) = launch;
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
    let cache = output.join("callback-cache");
    let mut body = request_body(
        data,
        [plan, resources, &persistent],
        &identity,
        launch,
        &cache,
    );
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
/// `screen` is the window size, listed as `screen` when there is a window. `plugins` are listed as
/// `plugins` when there are any. `callback_cache` is where the managed side keeps the player's
/// assemblies rewritten so the delegates they give native code stay callable.
fn request_body(
    data: &Path,
    paths: [&Path; 3],
    identity: &PlayerIdentity,
    launch: PlayerLaunch,
    callback_cache: &Path,
) -> serde_json::Value {
    let [plan, resources, persistent] = paths;
    let PlayerLaunch {
        arguments,
        screen,
        plugins,
    } = launch;
    let mut body = serde_json::json!({
        "player": data,
        "plan": plan,
        "resources": resources,
        "identity": identity,
        "persistent": persistent,
        "arguments": arguments,
        "first_texture": FIRST_TEXTURE,
        "callback_cache": callback_cache,
    });
    if let Some(screen) = screen {
        body["screen"] = serde_json::json!(screen);
    }
    if !plugins.is_empty() {
        body["plugins"] = serde_json::json!(plugins);
    }
    body
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

/// Whether a run does the player's offscreen work on a device.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GpuWork {
    /// Takes a device when the machine has an adapter, and runs without GPU work when it has
    /// none. A frame whose snapshot holds no work leaves the device alone.
    #[default]
    Auto,
    /// Never touches a device: the run is the one before the offscreen host existed.
    Off,
}

/// The environment variable that makes a headless run print the GPU report after every frame, not
/// only at the end.
const REPORT_EACH_FRAME: &str = "PLAY_REPORT_EACH_FRAME";

/// Boots the host, starts the player's scene and advances it `frames` times by `dt`, with the
/// player's offscreen work on a device when there is one ([`GpuWork::Auto`]). The error names the
/// step that failed: starting the scene or the frame that failed.
pub fn run_player(prepared: &PreparedPlayer, frames: u64, dt: Duration) -> Result<(), PlayError> {
    run_player_with(prepared, frames, dt, GpuWork::Auto)
}

/// [`run_player`] with `gpu` saying whether the offscreen work runs.
pub fn run_player_with(
    prepared: &PreparedPlayer,
    frames: u64,
    dt: Duration,
    gpu: GpuWork,
) -> Result<(), PlayError> {
    let mut host = DisunityHost::boot(&prepared.config)?;
    let gpu = match gpu {
        GpuWork::Off => None,
        GpuWork::Auto => PlayerGpu::headless(&prepared.data)
            .inspect_err(|reason| {
                eprintln!("[gpu] {reason}; the offscreen work does not run");
            })
            .ok(),
    };
    // Installed before the scene loads: a script may read a compute buffer back while it does.
    if let Some(gpu) = &gpu {
        gpu.install(&mut host)?;
    }
    host.run_slice(&prepared.request)?;
    let nanos = u64::try_from(dt.as_nanos())
        .map_err(|_| PlayError::Invalid("the frame delta is too long".into()))?;
    let mut result = Ok(());
    let mut meshes = PlayerMeshes::default();
    let mut times = Vec::new();
    for frame in 0..frames {
        let started = Instant::now();
        let stepped = host
            .advance(nanos, 1_000_000_000)
            .map_err(PlayError::from)
            .and_then(|()| present_meshes(&mut host, &mut meshes))
            .and_then(|()| gpu.as_ref().map_or(Ok(()), |gpu| gpu.frame(&mut host)));
        times.push(started.elapsed());
        // A run that dies in a later frame still leaves the counts of the frames before it.
        if std::env::var_os(REPORT_EACH_FRAME).is_some()
            && let Some(gpu) = &gpu
        {
            eprintln!("[gpu] report after frame {frame}");
            gpu.report().print();
        }
        if let Err(error) = stepped {
            result = Err(PlayError::Invalid(format!("frame {frame}: {error}")));
            break;
        }
    }
    if result.is_ok() {
        eprintln!("[player] ran {frames} frames");
    }
    report_times(&times);
    if let Some(gpu) = &gpu {
        gpu.report().print();
    }
    // A frame's failure says more than an unload failure that follows it.
    let unloaded = host.unload_slice();
    result.and(unloaded.map_err(PlayError::from))
}

/// Writes the first, mean and slowest frame time to standard error.
fn report_times(times: &[Duration]) {
    let Some(first) = times.first() else {
        return;
    };
    let total: Duration = times.iter().sum();
    let slowest = times.iter().max().copied().unwrap_or_default();
    let count = u32::try_from(times.len()).unwrap_or(u32::MAX);
    eprintln!(
        "[player] frame time: first {first:?}, mean {:?}, slowest {slowest:?}, {} frames",
        total / count,
        times.len()
    );
}

/// Prepares the frame's presentation at the size of a small window and brings its meshes in, so a
/// headless run reports what the player shows in 3D.
fn present_meshes(host: &mut DisunityHost, meshes: &mut PlayerMeshes) -> Result<(), PlayError> {
    host.export_presentation([960, 540], FIRST_COVERAGE_TEXTURE)?;
    let objects = host.mesh_objects()?;
    meshes.refresh(host, &objects)
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
        body_in(arguments, None)
    }

    fn body_in(arguments: &[String], screen: Option<[u32; 2]>) -> serde_json::Value {
        let (plan, resources, persistent) = (
            Path::new("plan.json"),
            Path::new("resources.json"),
            Path::new("persistent"),
        );
        request_body(
            Path::new("/games/Game/Game_Data"),
            [plan, resources, persistent],
            &identity(),
            PlayerLaunch {
                arguments,
                screen,
                plugins: &[],
            },
            Path::new("/out/callback-cache"),
        )
    }

    #[test]
    fn the_request_lists_the_plugin_directories_in_order() {
        let plugins = [PathBuf::from("/a/Plugins"), PathBuf::from("b")];
        let request = request_body(
            Path::new("/games/Game/Game_Data"),
            [Path::new("p"), Path::new("r"), Path::new("x")],
            &identity(),
            PlayerLaunch {
                plugins: &plugins,
                ..PlayerLaunch::default()
            },
            Path::new("/out/callback-cache"),
        );
        assert_eq!(request["plugins"], serde_json::json!(["/a/Plugins", "b"]));
    }

    #[test]
    fn the_request_names_the_directory_for_player_assemblies_rewritten_at_load() {
        assert_eq!(
            body(&[])["callback_cache"],
            serde_json::json!("/out/callback-cache")
        );
    }

    #[test]
    fn a_request_without_plugin_directories_names_none() {
        assert!(body(&[]).get("plugins").is_none());
    }

    #[test]
    fn the_request_names_the_window_size_the_player_reads_as_its_screen() {
        let request = body_in(&[], Some([960, 540]));
        assert_eq!(request["screen"], serde_json::json!([960, 540]));
    }

    #[test]
    fn a_headless_request_names_no_screen() {
        assert!(body(&[]).get("screen").is_none());
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
            Ok(PathBuf::from("out/scene-0.json")),
            Err("object 3 does not decode".to_owned()),
            Ok(PathBuf::from("out/scene-2.json")),
        ];
        assert_eq!(
            serde_json::Value::Array(scene_entries(&paths, &plans)),
            serde_json::json!([
                {"name": "StartScreen", "path": "Assets/GameAssets/Scenes/StartScreen.unity", "plan": "out/scene-0.json"},
                {"name": "MainGame", "path": "Assets/GameAssets/Scenes/MainGame.unity", "plan": null, "error": "object 3 does not decode"},
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
