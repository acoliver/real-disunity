//! Runs a built Unity player's scene headless in the managed host.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use worldrunner_disunity_runtime::{DisunityHost, ManagedHostConfig};
use worldrunner_original_games::workspace_root;
use worldrunner_unity_import::{
    player_identity::player_identity,
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
/// all into `<target>/play/player`.
pub fn prepare_player(data: &Path, scene: &str) -> Result<PreparedPlayer, PlayError> {
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
    write(
        &plan_path,
        &serde_json::to_vec(&plan).expect("plans serialize"),
    )?;
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
    let request = write_request(&output, data, &plan_path, &resources_path)?;
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

/// Writes the bridge's request: the player's paths, plans and identity. Each game keeps its
/// preferences in its own `persistent/<company>/<product>` directory under `output`.
fn write_request(
    output: &Path,
    data: &Path,
    plan: &Path,
    resources: &Path,
) -> Result<PathBuf, PlayError> {
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
    let body = serde_json::json!({
        "player": data,
        "plan": plan,
        "resources": resources,
        "identity": identity,
        "persistent": persistent,
        "first_texture": FIRST_TEXTURE,
    });
    write(&request, body.to_string().as_bytes())?;
    Ok(request)
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
