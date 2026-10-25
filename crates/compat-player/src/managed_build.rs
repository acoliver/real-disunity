//! Compiles the managed bridge and an original game's scripts against the Unity facade.
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use compat_kit::{files, model::RunRequest};
use worldrunner::managed::ManagedHostConfig;

use crate::{Installation, dotnet};

const BUILD_TIMEOUT: Duration = Duration::from_mins(2);
const MAX_SOURCE_BYTES: u64 = 1_073_741_824;
const MAX_SOURCE_FILES: usize = 100_000;

pub(crate) struct Assemblies {
    pub host: ManagedHostConfig,
    pub game: PathBuf,
}

pub(crate) fn prepare(
    installation: &Installation,
    request: &RunRequest,
) -> Result<Assemblies, String> {
    let root = installation.root();
    let configuration = Configuration::from_request(request)?;
    let game = compile_game(installation, request)?;
    let build = build_directory(root, request)?;
    let bridge_project = root.join("csharp/CompatibilityBridge/CompatibilityBridge.csproj");
    build_project(root, &bridge_project, &build.join("bridge"), configuration)?;
    let bridge = build.join(format!(
        "bridge/bin/CompatibilityBridge/{}/CompatibilityBridge.dll",
        configuration.output()
    ));
    let native = crate::native_image::process_copy(root, installation.native_library())?;
    Ok(Assemblies {
        host: ManagedHostConfig {
            ffi_library: native,
            runtime_config: bridge.with_extension("runtimeconfig.json"),
            assembly: bridge,
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        },
        game,
    })
}

/// Compiles every C# script under the request's `Assets/Scripts` against the facade and returns
/// the gameplay assembly. No game code runs.
pub fn compile_game(installation: &Installation, request: &RunRequest) -> Result<PathBuf, String> {
    let root = installation.root();
    let configuration = Configuration::from_request(request)?;
    let build = build_directory(root, request)?;
    let sources = gameplay_sources(request)?;
    let project = build.join("OriginalGame.csproj");
    fs::write(&project, game_project(root, &sources)).map_err(files::io)?;
    let output = build.join("game");
    build_project(root, &project, &output, configuration)?;
    let assembly = output.join(format!(
        "bin/OriginalGame/{}/OriginalGame.dll",
        configuration.output()
    ));
    if !assembly.is_file() {
        return Err("compiler did not produce the original gameplay assembly".into());
    }
    Ok(assembly)
}

fn build_directory(root: &Path, request: &RunRequest) -> Result<PathBuf, String> {
    if !request.artifact_root.is_absolute() {
        return Err("artifact_root must be absolute".into());
    }
    let build = files::work_path(root, &request.artifact_root.join("build"))?;
    fs::create_dir_all(&build).map_err(files::io)?;
    Ok(build)
}

fn gameplay_sources(request: &RunRequest) -> Result<Vec<PathBuf>, String> {
    let source = &request.source_root;
    if !source.is_absolute() || source.canonicalize().map_err(files::io)? != *source {
        return Err("source_root must be absolute, canonical and free of symlinks".into());
    }
    let directory = source.join("Assets/Scripts");
    if directory.canonicalize().map_err(files::io)? != directory {
        return Err("source directory symlink rejected".into());
    }
    let sources: Vec<PathBuf> = files::tree_files(&directory, MAX_SOURCE_BYTES, MAX_SOURCE_FILES)?
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("cs"))
        })
        .collect();
    if sources.is_empty() {
        return Err("original gameplay compilation has no C# files".into());
    }
    expect_script_count(request, sources.len())?;
    Ok(sources)
}

fn expect_script_count(request: &RunRequest, actual: usize) -> Result<(), String> {
    let Some(value) = request.metadata.get("script_count") else {
        return Ok(());
    };
    let expected = value
        .as_u64()
        .filter(|count| *count > 0)
        .ok_or("script_count must be a positive integer")?;
    if u64::try_from(actual).map_err(files::io)? != expected {
        return Err(format!(
            "expected {expected} original scripts, found {actual}"
        ));
    }
    Ok(())
}

fn game_project(root: &Path, sources: &[PathBuf]) -> String {
    let mut includes = String::new();
    for path in sources {
        includes.push_str("<Compile Include=\"");
        includes.push_str(&escape(path));
        includes.push_str("\" />");
    }
    let facade = escape(&root.join("csharp/UnityCompat/UnityCompat.csproj"));
    format!(
        r#"<Project Sdk="Microsoft.NET.Sdk">
<PropertyGroup><TargetFramework>net8.0</TargetFramework><EnableDefaultCompileItems>false</EnableDefaultCompileItems><ImplicitUsings>disable</ImplicitUsings><GenerateAssemblyInfo>false</GenerateAssemblyInfo><GenerateTargetFrameworkAttribute>false</GenerateTargetFrameworkAttribute><AssemblyName>OriginalGame</AssemblyName></PropertyGroup>
<ItemGroup><ProjectReference Include="{facade}" />{includes}</ItemGroup>
</Project>"#
    )
}

fn escape(path: &Path) -> String {
    path.to_string_lossy()
        .replace('%', "%25")
        .replace(';', "%3B")
        .replace('*', "%2A")
        .replace('?', "%3F")
        .replace('$', "%24")
        .replace('@', "%40")
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn build_project(
    root: &Path,
    project: &Path,
    artifacts: &Path,
    configuration: Configuration,
) -> Result<(), String> {
    let mut command = dotnet::command(root);
    command
        .arg("build")
        .arg(project)
        .arg("--artifacts-path")
        .arg(artifacts)
        .args(["--nologo", "-m:2", "-t:Rebuild", "-c", configuration.name()]);
    let log = artifacts.with_extension("build.log");
    dotnet::run(&mut command, &log, BUILD_TIMEOUT).map_err(|error| {
        format!(
            "managed compilation failed for {}: {error}",
            project.display()
        )
    })
}

/// Managed compiler configuration for the bridge, facade and game, selected by the fixture's
/// `managed_configuration` metadata. Debug is the default; Release changes only optimization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Configuration {
    Debug,
    Release,
}

impl Configuration {
    fn from_request(request: &RunRequest) -> Result<Self, String> {
        match request.metadata.get("managed_configuration") {
            None => Ok(Self::Debug),
            Some(value) => Self::parse(
                value
                    .as_str()
                    .ok_or("managed_configuration must be a string")?,
            ),
        }
    }
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "Debug" => Ok(Self::Debug),
            "Release" => Ok(Self::Release),
            other => Err(format!(
                "managed_configuration must be Debug or Release, got {other:?}"
            )),
        }
    }
    const fn name(self) -> &'static str {
        match self {
            Self::Debug => "Debug",
            Self::Release => "Release",
        }
    }
    const fn output(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_configuration_is_explicit_and_rejects_unknown_values() {
        assert_eq!(Configuration::parse("Debug"), Ok(Configuration::Debug));
        assert_eq!(Configuration::parse("Release"), Ok(Configuration::Release));
        for invalid in ["", "release", "debug", "Release ", "Checked"] {
            assert!(Configuration::parse(invalid).is_err(), "{invalid:?}");
        }
        assert_eq!(Configuration::Release.output(), "release");
        assert_eq!(Configuration::Debug.name(), "Debug");
    }

    #[test]
    fn project_compiles_exactly_the_selected_sources_against_the_facade() {
        let root = Path::new("/workspace");
        let project = game_project(root, &[PathBuf::from("/game/Assets/Scripts/A&B.cs")]);
        assert!(project.contains("<Compile Include=\"/game/Assets/Scripts/A&amp;B.cs\" />"));
        assert!(project.contains("/workspace/csharp/UnityCompat/UnityCompat.csproj"));
        assert!(project.contains("<EnableDefaultCompileItems>false</EnableDefaultCompileItems>"));
    }
}
