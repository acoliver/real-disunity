//! Everything a play session needs before the host boots.
use std::path::{Path, PathBuf};
use std::process::Command;

use worldrunner_disunity_runtime::ManagedHostConfig;
use worldrunner_original_games::{Game, checkout, workspace_root};
use worldrunner_render_plan::simulation::SpriteTextures;
use worldrunner_unity_import::{Project, plan_scene};

use crate::{PlayError, scene, textures::scene_textures};

/// Plan textures take identities from here up, clear of the glyph coverage textures the
/// presentation allocates from [`crate::driver`]'s range.
pub(crate) const FIRST_TEXTURE: u64 = 1;

/// A game ready to boot: the host configuration, the request its bridge loads and the
/// scene's decoded textures.
pub struct Prepared {
    pub game: Game,
    pub config: ManagedHostConfig,
    pub request: PathBuf,
    pub textures: SpriteTextures,
}

/// Fetches `game`, plans its scene and builds its scripts and the managed bridge into
/// `<target>/play/<game>`. `font` supplies Unity's Arial; on Linux it defaults to the font
/// `fc-match Arial` selects. A `seed` makes the game's `Random` repeat its sequence.
pub fn prepare(game: Game, font: Option<&Path>, seed: Option<i32>) -> Result<Prepared, PlayError> {
    let root = workspace_root()?;
    let output = target(&root).join("play").join(game.name());
    std::fs::create_dir_all(&output).map_err(|error| PlayError::Io(output.clone(), error))?;
    let source = checkout(game)?;
    let project = Project::open(&source)?;
    let plan = plan_scene(&project, Path::new(scene(game)))?;
    let plan_path = output.join("plan.json");
    write(
        &plan_path,
        &serde_json::to_vec(&plan).expect("plans serialize"),
    )?;
    let textures = scene_textures(&project, &plan.textures, FIRST_TEXTURE)?;
    let fonts = fonts(
        &serde_json::to_value(&plan.fonts).expect("fonts serialize"),
        font,
    )?;
    let (scripts, bridge) = build(&root, game, &source, &output)?;
    let request = output.join("request.json");
    let mut body = serde_json::json!({
        "plan": plan_path,
        "scripts": scripts,
        "fonts": fonts,
        "first_texture": FIRST_TEXTURE,
    });
    if let Some(seed) = seed {
        body["seed"] = seed.into();
    }
    write(&request, body.to_string().as_bytes())?;
    let config = ManagedHostConfig {
        ffi_library: native_library()?,
        runtime_config: bridge.join("Disunity.Play.runtimeconfig.json"),
        assembly: bridge.join("Disunity.Play.dll"),
        bridge_type: "Disunity.Play.PlayBridge, Disunity.Play".into(),
    };
    Ok(Prepared {
        game,
        config,
        request,
        textures,
    })
}

/// The font file for each package font the plan names.
fn fonts(
    names: &serde_json::Value,
    font: Option<&Path>,
) -> Result<serde_json::Map<String, serde_json::Value>, PlayError> {
    let mut files = serde_json::Map::new();
    for name in names
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
    {
        let file = font.map_or_else(|| system_font(name), |file| Ok(file.to_owned()))?;
        files.insert(name.to_owned(), file.display().to_string().into());
    }
    Ok(files)
}

/// Builds the game's scripts and the bridge, returning the scripts assembly and the bridge
/// directory.
fn build(
    root: &Path,
    game: Game,
    source: &Path,
    output: &Path,
) -> Result<(PathBuf, PathBuf), PlayError> {
    let name = title(game);
    let scripts = output.join("scripts");
    dotnet(&[
        root.join(format!("csharp/OriginalGames/{name}/{name}.csproj"))
            .as_os_str(),
        format!("-p:GameRoot={}", source.display()).as_ref(),
        "-o".as_ref(),
        scripts.as_os_str(),
    ])?;
    let bridge = output.join("bridge");
    dotnet(&[
        root.join("csharp/Disunity.Play/Disunity.Play.csproj")
            .as_os_str(),
        "-o".as_ref(),
        bridge.as_os_str(),
    ])?;
    Ok((scripts.join(format!("Original.{name}.dll")), bridge))
}

fn title(game: Game) -> &'static str {
    match game {
        Game::Snake => "Snake",
        Game::Breakout => "Breakout",
        Game::Pong => "Pong",
    }
}

fn target(root: &Path) -> PathBuf {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    if target.is_absolute() {
        target
    } else {
        root.join(target)
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), PlayError> {
    std::fs::write(path, bytes).map_err(|error| PlayError::Io(path.to_owned(), error))
}

fn run(command: &mut Command) -> Result<String, PlayError> {
    let output = command
        .output()
        .map_err(|error| PlayError::Invalid(format!("{command:?}: {error}")))?;
    if !output.status.success() {
        return Err(PlayError::Invalid(format!(
            "{command:?} failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn dotnet(args: &[&std::ffi::OsStr]) -> Result<(), PlayError> {
    let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    run(Command::new(dotnet)
        .args(["build", "--warnaserror", "--nologo", "-v", "quiet"])
        .args(args))
    .map(drop)
}

/// The installed font that stands in for a Unity package font.
fn system_font(name: &str) -> Result<PathBuf, PlayError> {
    let missing = || {
        PlayError::Invalid(format!(
            "the scene uses Unity's {name} font; pass a TrueType file with --font"
        ))
    };
    if !cfg!(target_os = "linux") {
        return Err(missing());
    }
    let file =
        run(Command::new("fc-match").args(["--format=%{file}", name])).map_err(|_| missing())?;
    let file = PathBuf::from(file.trim());
    let truetype = file
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ttf"));
    if truetype { Ok(file) } else { Err(missing()) }
}

/// The engine library: `DISUNITY_FFI_LIBRARY`, or the one Cargo built beside this program.
fn native_library() -> Result<PathBuf, PlayError> {
    if let Some(path) = std::env::var_os("DISUNITY_FFI_LIBRARY") {
        return Ok(PathBuf::from(path));
    }
    let name = format!(
        "{}worldrunner_disunity_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let exe = std::env::current_exe()
        .map_err(|error| PlayError::Io("current executable".into(), error))?;
    exe.ancestors()
        .skip(1)
        .take(2)
        .map(|directory| directory.join(&name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            PlayError::Invalid(format!(
                "{name} not found beside {}; build it with `cargo build -p worldrunner-disunity-ffi` or set DISUNITY_FFI_LIBRARY",
                exe.display()
            ))
        })
}
