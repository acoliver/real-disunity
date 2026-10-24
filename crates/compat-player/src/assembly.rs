use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use compat_kit::{files, model::RunRequest};
use worldrunner::managed::ManagedHostConfig;

use crate::{assembly_probe, assembly_qualification};

pub(crate) struct Assemblies {
    pub host: ManagedHostConfig,
    pub game: PathBuf,
}

pub(crate) fn prepare(root: &Path, request: &RunRequest) -> Result<Assemblies, String> {
    let native_source = native_library(root)?;
    let game = assembly_qualification::prepare_game(root, request)?;
    let build = assembly_qualification::build_directory(root, request)?;
    let bridge_project = root.join("csharp/CompatibilityBridge/CompatibilityBridge.csproj");
    build_project(root, &bridge_project, &build.join("bridge"))?;
    let bridge = build.join(format!(
        "bridge/bin/CompatibilityBridge/{}/CompatibilityBridge.dll",
        configuration()?.output()
    ));
    let runtime_config = bridge.with_extension("runtimeconfig.json");
    let native = crate::process_image::admit(root, &native_source, &build)?;
    Ok(Assemblies {
        host: ManagedHostConfig {
            ffi_library: native,
            assembly: bridge,
            runtime_config,
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        },
        game,
    })
}

#[cfg(test)]
fn native_library(_root: &Path) -> Result<PathBuf, String> {
    let selected = crate::managed_test_artifacts::artifacts().native.clone();
    crate::native_image::source_identity(&selected)?;
    Ok(selected)
}

#[cfg(not(test))]
fn native_library(root: &Path) -> Result<PathBuf, String> {
    let configured = std::env::var_os("COMPAT_NATIVE_LIBRARY").map(PathBuf::from);
    crate::provider::select(root, configured.as_deref())
}

pub(crate) fn compile_game(
    root: &Path,
    request: &RunRequest,
    build: &Path,
    sources: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let proof = build.join("original-source-count.json");
    if proof.exists() {
        fs::remove_file(&proof).map_err(files::io)?;
    }
    let includes = sources
        .keys()
        .map(|path| {
            format!(
                "<Compile Include=\"{}\" />",
                escape(&request.source_root.join(path))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let facade = escape(&root.join("csharp/UnityCompat/UnityCompat.csproj"));
    let manifest_path = build.join("actual-compile-items.txt");
    if manifest_path.exists() {
        fs::remove_file(&manifest_path).map_err(files::io)?;
    }
    let manifest = escape(&manifest_path);
    let project = build.join("OriginalGame.csproj");
    fs::write(&project, format!(r#"<Project Sdk="Microsoft.NET.Sdk">
<PropertyGroup><TargetFramework>net8.0</TargetFramework><EnableDefaultCompileItems>false</EnableDefaultCompileItems><ImplicitUsings>disable</ImplicitUsings><GenerateAssemblyInfo>false</GenerateAssemblyInfo><GenerateTargetFrameworkAttribute>false</GenerateTargetFrameworkAttribute><AssemblyName>OriginalGame</AssemblyName></PropertyGroup>
<ItemGroup><ProjectReference Include="{facade}" />{includes}</ItemGroup>
<Target Name="RecordOriginalSources" BeforeTargets="CoreCompile"><WriteLinesToFile File="{manifest}" Lines="@(Compile->'%(FullPath)')" Overwrite="true" /></Target>
</Project>"#)).map_err(files::io)?;
    let output = build.join("game");
    build_project(root, &project, &output)?;
    let assembly = output.join(format!(
        "bin/OriginalGame/{}/OriginalGame.dll",
        configuration()?.output()
    ));
    if !assembly.is_file() {
        return Err("compiler did not produce the original gameplay assembly".into());
    }
    assembly_qualification::record_proof(root, request, build, &assembly, sources)?;
    Ok(assembly)
}

/// Managed compiler configuration for the bridge, facade and game. Debug remains the default;
/// Release is an explicit opt-in and changes only compiler optimization, not source or semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Configuration {
    Debug,
    Release,
}

impl Configuration {
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
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("Debug") => Ok(Self::Debug),
            Some("Release") => Ok(Self::Release),
            Some(other) => Err(format!(
                "COMPAT_MANAGED_CONFIGURATION must be Debug or Release, got {other:?}"
            )),
        }
    }
}

fn configuration() -> Result<Configuration, String> {
    let value = std::env::var_os("COMPAT_MANAGED_CONFIGURATION");
    let value = value
        .as_deref()
        .map(|v| {
            v.to_str()
                .ok_or("COMPAT_MANAGED_CONFIGURATION must be UTF-8")
        })
        .transpose()?;
    Configuration::parse(value)
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

fn build_project(root: &Path, project: &Path, artifacts: &Path) -> Result<(), String> {
    assembly_qualification::work_path(root, artifacts)?;
    let mut command = assembly_probe::dotnet(root);
    command
        .arg("build")
        .arg(project)
        .arg("--artifacts-path")
        .arg(artifacts)
        .args([
            "--nologo",
            "-m:2",
            "-t:Rebuild",
            "-c",
            configuration()?.name(),
        ]);
    let log = artifacts.with_extension("build.log");
    assembly_probe::run(&mut command, &log, Duration::from_mins(2)).map_err(|error| {
        format!(
            "managed compilation failed for {}: {error}",
            project.display()
        )
    })
}

#[cfg(test)]
mod configuration_tests {
    use super::Configuration;

    #[test]
    fn managed_configuration_defaults_to_debug_and_rejects_unknown_values() {
        assert_eq!(Configuration::parse(None), Ok(Configuration::Debug));
        assert_eq!(
            Configuration::parse(Some("Debug")),
            Ok(Configuration::Debug)
        );
        assert_eq!(
            Configuration::parse(Some("Release")),
            Ok(Configuration::Release)
        );
        for invalid in ["", "release", "debug", "Release ", "Checked"] {
            assert!(Configuration::parse(Some(invalid)).is_err(), "{invalid:?}");
        }
        assert_eq!(Configuration::Release.output(), "release");
        assert_eq!(Configuration::Debug.name(), "Debug");
    }
}
