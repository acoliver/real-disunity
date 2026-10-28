//! Runs a built Unity player's scene headless in the managed host.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use worldrunner_disunity_runtime::{DisunityHost, ManagedHostConfig};
use worldrunner_original_games::workspace_root;
use worldrunner_unity_import::{
    player_plan::plan_player_scene, script_layouts::parse_script_layouts,
};

use crate::PlayError;
use crate::prepare::{FIRST_TEXTURE, bridge_config, dotnet, run, target, write};

/// A player scene ready to boot: the host configuration and the request its bridge loads.
pub struct PreparedPlayer {
    pub config: ManagedHostConfig,
    pub request: PathBuf,
}

/// Writes the script layouts of the player in `data` (its `<Game>_Data` directory), plans
/// scene file `scene` with them and builds the managed bridge, all into
/// `<target>/play/player`.
pub fn prepare_player(data: &Path, scene: &str) -> Result<PreparedPlayer, PlayError> {
    let root = workspace_root()?;
    let output = target(&root).join("play").join("player");
    std::fs::create_dir_all(&output).map_err(|error| PlayError::Io(output.clone(), error))?;
    let tool = output.join("layouts-tool");
    dotnet(&[
        root.join("csharp/Disunity.Layouts/Disunity.Layouts.csproj")
            .as_os_str(),
        "-o".as_ref(),
        tool.as_os_str(),
    ])?;
    let layouts_path = output.join("layouts.json");
    let summary = run(Command::new(dotnet_host())
        .arg(tool.join("Disunity.Layouts.dll"))
        .arg(data)
        .arg(&layouts_path))?;
    eprintln!("[player] script layouts: {}", summary.trim());
    let json = std::fs::read_to_string(&layouts_path)
        .map_err(|error| PlayError::Io(layouts_path.clone(), error))?;
    let layouts = parse_script_layouts(&json)?;
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
    let request = output.join("request.json");
    let body = serde_json::json!({
        "player": data,
        "plan": plan_path,
        "first_texture": FIRST_TEXTURE,
    });
    write(&request, body.to_string().as_bytes())?;
    Ok(PreparedPlayer {
        config: bridge_config(&root, &output)?,
        request,
    })
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
    host.unload_slice()?;
    result
}

fn dotnet_host() -> std::ffi::OsString {
    std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into())
}
