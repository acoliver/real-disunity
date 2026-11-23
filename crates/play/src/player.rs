//! Runs a built Unity player's scene headless in the managed host.
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use std::time::Instant;

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_disunity_runtime::ManagedHostConfig;
use worldrunner_original_games::workspace_root;
use worldrunner_unity_import::player_identity::PlayerIdentity;
use worldrunner_unity_import::player_identity::player_identity;
use worldrunner_unity_import::player_identity::player_project_graphics;
use worldrunner_unity_import::player_identity::player_quality;
use worldrunner_unity_import::player_identity::player_scene_paths;
use worldrunner_unity_import::player_plan::plan_player_scene;
use worldrunner_unity_import::player_resources::plan_player_resources;
use worldrunner_unity_import::script_layouts::parse_script_layouts;
use worldrunner_unity_serialized::script_data::ScriptLayouts;

use crate::PlayError;
use crate::driver::FIRST_COVERAGE_TEXTURE;
use crate::gpu_player::GpuLaunch;
use crate::gpu_player::PlayerGpu;
use crate::offscreen::StandInLook;
use crate::offscreen::SuppliedGlobal;
use crate::plan_differences::shader_difference_lines;
use crate::player_meshes::PlayerMeshes;
use crate::prepare::FIRST_TEXTURE;
use crate::prepare::bridge_config;
use crate::prepare::dotnet;
use crate::prepare::run;
use crate::prepare::target;
use crate::prepare::write;
use crate::target_read::TargetRead;

/// A player scene ready to boot: the host configuration and the request its bridge loads.
pub struct PreparedPlayer {
    pub config: ManagedHostConfig,
    pub request: PathBuf,
    /// The player's `<Game>_Data` directory, where its shaders and serialized files are read.
    pub data: PathBuf,
    /// The size the player reads as its screen, which a headless run presents its frames at and
    /// draws its screen texture in; `None` when the launch gave none.
    pub screen: Option<[u32; 2]>,
    /// What the run asks of its GPU work: the globals it supplies, the stand-in look, whether it
    /// reports after every frame and the target reads.
    pub gpu: GpuLaunch,
}

/// How a player is started besides its files: the command line after its executable path,
/// the size of the window the frame is drawn into, which is the size the player's `Screen`
/// reports (a run with no viewer gives none unless it reads targets), and extra native plugin directories the player
/// searches after its own `Plugins` directories and the engine's substitutes. `globals` are the
/// values `--global` supplies; they reach the GPU work only, not the request, and so do
/// `stand_ins` (how the stand-in textures look), `report_each_frame` and `read_targets` (the
/// records whose targets a headless run writes to files). The default has no arguments, no
/// window, no extra plugins and no globals, and the declared stand-ins and a report at the end
/// only.
#[derive(Clone, Copy, Default)]
pub struct PlayerLaunch<'a> {
    pub arguments: &'a [String],
    pub screen: Option<[u32; 2]>,
    pub plugins: &'a [PathBuf],
    pub globals: &'a [SuppliedGlobal],
    pub stand_ins: StandInLook,
    pub report_each_frame: bool,
    pub read_targets: &'a [TargetRead],
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
    let differences: Vec<String> = [&plan.differences, &resources.differences]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    for line in shader_difference_lines(&differences) {
        eprintln!("{line}");
    }
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
        screen: launch.screen,
        gpu: GpuLaunch {
            globals: launch.globals.to_vec(),
            stand_ins: launch.stand_ins,
            report_each_frame: launch.report_each_frame,
            reads: launch.read_targets.to_vec(),
        },
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

/// Writes the bridge's request: the player's paths, plans, identity, command line arguments,
/// quality levels and project graphics settings. Each game keeps its preferences in its own
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
    if let Some(project) = player_project_graphics(data)? {
        body["project"] = serde_json::json!(project);
    }
    write(&request, body.to_string().as_bytes())?;
    Ok(request)
}

/// The bridge's request. `paths` are the scene plan, the resource plan and the persistent data
/// directory. The managed side derives the player's executable path from `player` and the
/// platform in `identity`, and puts `arguments` after it in `Environment.GetCommandLineArgs()`.
/// `screen` is the window size, listed as `screen` when there is a window. `plugins` are listed as
/// `plugins` when there are any. `callback_cache` is where the managed side keeps the player's
/// assemblies rewritten so the delegates they give native code stay callable. `render_formats` are
/// the `GraphicsFormat` codes the offscreen renderer draws into and samples, which the player's
/// device reports as supported. `shadow_sampling` says the offscreen renderer reads a depth render
/// texture through a comparison sampler, which the player's device reports as shadow support.
/// `reversed_z_buffer` is true: the run's GPU work draws with reversed depth, so the player's
/// device declares a reversed depth buffer.
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
        ..
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
        "render_formats": worldrunner_render_program::unity::offscreen::RENDER_FORMATS,
        "shadow_sampling": true,
        "reversed_z_buffer": true,
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

/// The device and offscreen host a run does its GPU work on. Shaders are loaded from the player's
/// `data` directory.
///
/// # Errors
/// Fails naming the reason when the machine has no adapter or the device cannot be had.
pub(crate) fn player_gpu(prepared: &PreparedPlayer) -> Result<PlayerGpu, PlayError> {
    PlayerGpu::headless(&prepared.data, &prepared.gpu).map_err(|reason| gpu_unavailable(&reason))
}

/// The device and offscreen host a window does its GPU work on, on the device `source` makes for
/// the viewer. Shaders are loaded from the player's `data` directory, and the work is set up from
/// `launch` as a headless run's is ([`player_gpu`]).
///
/// # Errors
/// Fails naming the reason when `source` has no adapter or cannot make the device.
pub(crate) fn window_gpu(
    data: &Path,
    launch: &GpuLaunch,
    source: &mut dyn crate::gpu_device::DeviceSource,
) -> Result<PlayerGpu, PlayError> {
    PlayerGpu::shared(source, data, launch).map_err(|reason| gpu_unavailable(&reason))
}

fn gpu_unavailable(reason: &str) -> PlayError {
    PlayError::Invalid(format!("the player's offscreen work needs a GPU: {reason}"))
}

/// Boots the host, starts the player's scene and advances it `frames` times by `dt`, with the
/// player's offscreen work on a device and no viewer: the run measures the game and its GPU work
/// and shows nothing. It presents its frames at the screen size `prepared` carries, else at a small
/// window's, and writes each `--read-target` read its frame names. The error names the step that
/// failed: the missing GPU, starting the scene or the frame that failed. A run that writes frames
/// is a windowless run ([`play_player_windowless`](crate::play_player_windowless)).
///
/// # Errors
/// Fails naming the reason when the machine has no GPU adapter or device, when the scene cannot
/// start, when a read names a frame the run does not play, and at a frame that fails, including a
/// frame or flush the GPU refuses (the error names the refusal).
pub fn run_player(prepared: &PreparedPlayer, frames: u64, dt: Duration) -> Result<(), PlayError> {
    let mut host = DisunityHost::boot(&prepared.config)?;
    let gpu = player_gpu(prepared)?;
    TargetRead::check_frames(&prepared.gpu.reads, frames).map_err(PlayError::Invalid)?;
    gpu.set_reads(prepared.gpu.reads.clone());
    // Installed before the scene loads: a script may read a compute buffer back while it does.
    gpu.install(&mut host)?;
    gpu.set_screen(prepared.screen);
    host.run_slice(&prepared.request)?;
    let nanos = u64::try_from(dt.as_nanos())
        .map_err(|_| PlayError::Invalid("the frame delta is too long".into()))?;
    let mut result = Ok(());
    let mut meshes = PlayerMeshes::default();
    let viewport = prepared.screen.unwrap_or(PRESENTED_SIZE);
    let mut times = Vec::new();
    for frame in 0..frames {
        let stand_ins_before = gpu.report().stand_ins.clone();
        let started = Instant::now();
        let stepped = host
            .advance(nanos, 1_000_000_000)
            .map_err(PlayError::from)
            .and_then(|()| present_meshes(&mut host, &mut meshes, viewport))
            .and_then(|()| gpu.frame(&mut host))
            .and_then(|()| match gpu.take_refusal() {
                Some(refusal) => Err(PlayError::Invalid(refusal)),
                None => Ok(()),
            });
        times.push(started.elapsed());
        // A run that dies in a later frame still leaves the counts of the frames before it. The
        // counts are the run's so far, except the stand-in census, which is the frame's own.
        if prepared.gpu.report_each_frame {
            eprintln!("[gpu] report after frame {frame}");
            gpu.report().print(Some(&stand_ins_before));
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
    gpu.report().print(None);
    let unwritten = gpu.report().reads_unwritten;
    if result.is_ok() && unwritten > 0 {
        result = Err(PlayError::Invalid(format!(
            "{unwritten} --read-target reads wrote no file; the errors above name each"
        )));
    }
    // A frame's failure says more than an unload failure that follows it.
    let unloaded = host.unload_slice();
    result.and(unloaded.map_err(PlayError::from))
}

/// The line that gives the first, mean and slowest frame time, which the run's scripts read.
fn frame_time_line(times: &[Duration]) -> Option<String> {
    let first = times.first()?;
    let total: Duration = times.iter().sum();
    let slowest = times.iter().max().copied().unwrap_or_default();
    let count = u32::try_from(times.len()).unwrap_or(u32::MAX);
    Some(format!(
        "[player] frame time: first {first:?}, mean {:?}, slowest {slowest:?}, {} frames",
        total / count,
        times.len()
    ))
}

/// Writes the first, mean and slowest frame time to standard error.
pub(crate) fn report_times(times: &[Duration]) {
    if let Some(line) = frame_time_line(times) {
        eprintln!("{line}");
    }
}

/// The size a headless run presents its frames at: a small window's.
const PRESENTED_SIZE: [u32; 2] = [960, 540];

/// Prepares the frame's presentation at `viewport` and brings its meshes in, so a headless run
/// reports what the player shows in 3D.
fn present_meshes(
    host: &mut DisunityHost,
    meshes: &mut PlayerMeshes,
    viewport: [u32; 2],
) -> Result<(), PlayError> {
    host.export_presentation(viewport, FIRST_COVERAGE_TEXTURE)?;
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
                ..PlayerLaunch::default()
            },
            Path::new("/out/callback-cache"),
        )
    }

    #[test]
    fn the_frame_time_line_gives_the_first_mean_and_slowest_frame_and_their_count() {
        let times = [30, 10, 20].map(Duration::from_millis);
        assert_eq!(
            frame_time_line(&times).as_deref(),
            Some("[player] frame time: first 30ms, mean 20ms, slowest 30ms, 3 frames")
        );
        assert_eq!(frame_time_line(&[]), None);
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
    fn the_request_lists_the_formats_the_offscreen_renderer_draws_into() {
        assert_eq!(
            body(&[])["render_formats"],
            serde_json::json!(worldrunner_render_program::unity::offscreen::RENDER_FORMATS)
        );
    }

    #[test]
    fn the_request_says_the_device_samples_depth_with_a_comparison() {
        assert_eq!(body(&[])["shadow_sampling"], serde_json::json!(true));
    }

    #[test]
    fn a_built_player_declares_a_reversed_depth_buffer() {
        assert_eq!(body(&[])["reversed_z_buffer"], serde_json::json!(true));
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
