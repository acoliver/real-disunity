use compat_kit::files;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const PROGRAM: &str = r#"using System;
using System.IO;
using System.Linq;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Text.Json;
using var input = File.OpenRead(args[0]);
using var pe = new PEReader(input);
if (!pe.HasMetadata || pe.PEHeaders.CorHeader == null ||
    (pe.PEHeaders.CorHeader.Flags & CorFlags.ILOnly) == 0)
    throw new Exception("candidate has no IL-only managed metadata");
var metadata = pe.GetMetadataReader();
if (!metadata.IsAssembly) throw new Exception("candidate is not a CLI assembly");
var types = metadata.TypeDefinitions.Select(h => metadata.GetString(metadata.GetTypeDefinition(h).Name))
    .Where(n => n != "<Module>").ToArray();
if (types.Length == 0) throw new Exception("candidate defines no original types");
var references = metadata.AssemblyReferences.Select(h => metadata.GetString(metadata.GetAssemblyReference(h).Name)).ToArray();
if (!references.Contains("UnityCompat")) throw new Exception("candidate has no UnityCompat reference");
Console.WriteLine(JsonSerializer.Serialize(new { assembly = metadata.GetString(metadata.GetAssemblyDefinition().Name), types, references }));
"#;

pub(crate) fn inspect(root: &Path, build: &Path, assembly: &Path) -> Result<Value, String> {
    let directory = build.join("metadata-inspector");
    fs::create_dir_all(&directory).map_err(files::io)?;
    let project = directory.join("Inspector.csproj");
    fs::write(&project, r#"<Project Sdk="Microsoft.NET.Sdk">
<PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><EnableDefaultCompileItems>false</EnableDefaultCompileItems></PropertyGroup>
<ItemGroup><Compile Include="Program.cs" /></ItemGroup></Project>"#).map_err(files::io)?;
    fs::write(directory.join("Program.cs"), PROGRAM).map_err(files::io)?;
    let artifacts = build.join("metadata-artifacts");
    let mut compiler = dotnet(root);
    compiler
        .arg("build")
        .arg(&project)
        .arg("--artifacts-path")
        .arg(&artifacts)
        .args(["--nologo", "-m:2", "-t:Rebuild"]);
    run(
        &mut compiler,
        &build.join("metadata-build.log"),
        Duration::from_mins(2),
    )?;
    let mut probe = dotnet(root);
    probe
        .arg(artifacts.join("bin/Inspector/debug/Inspector.dll"))
        .arg(assembly);
    let log = build.join("metadata-inspection.log");
    run(&mut probe, &log, Duration::from_secs(15))
        .map_err(|error| format!("candidate managed metadata inspection failed: {error}"))?;
    files::read_json(&log)
}

pub(crate) fn dotnet(root: &Path) -> Command {
    let mut command = Command::new(root.join(".work/tools/dotnet/dotnet"));
    command.current_dir(root);
    command
}

pub(crate) fn run(command: &mut Command, log: &Path, timeout: Duration) -> Result<(), String> {
    let output = fs::File::create(log).map_err(files::io)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(files::io)?)
        .stderr(output)
        .spawn()
        .map_err(|error| format!("managed tool could not start: {error}"))?;
    let started = Instant::now();
    loop {
        let status = child.try_wait().map_err(files::io)?;
        let oversized = fs::metadata(log).map_err(files::io)?.len() > 4_194_304;
        if oversized || started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "managed tool exceeded time/log budget: {}",
                log.display()
            ));
        }
        if let Some(status) = status {
            if status.success() {
                return Ok(());
            }
            return Err(format!(
                "managed tool failed ({status}); see {}",
                log.display()
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}
