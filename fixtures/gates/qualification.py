"""Source selection and compile-proof verification for the qualification gate."""
import hashlib
from pathlib import Path


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def select_sources(source, selection):
    if not selection or len(set(selection)) != len(selection):
        raise RuntimeError("source selection must be explicit, nonempty and unique")
    for relative in selection:
        path = Path(relative)
        if path.is_absolute() or ".." in path.parts or path.suffix != ".cs":
            raise RuntimeError("unsafe source selection: " + relative)
        current = source
        for part in path.parts:
            current = current / part
            if current.is_symlink():
                raise RuntimeError("source symlink rejected: " + relative)
        if not current.is_file():
            raise RuntimeError("source selection file missing: " + relative)
    discovered = {str(p.relative_to(source)) for p in (source / "Assets/Scripts").rglob("*.cs")}
    if not set(selection).issubset(discovered):
        raise RuntimeError("source selection mismatch: " + str(sorted(set(selection) - discovered)))
    for path in (source / "Assets/Scripts").rglob("*"):
        if path.is_symlink():
            raise RuntimeError("source symlink rejected: " + str(path))
    return {relative: digest(source / relative) for relative in sorted(discovered)}


def verify_sources(source, selection, expected):
    if select_sources(source, selection) != expected:
        raise RuntimeError("source changed since compilation/qualification")


def inspect_assembly(root, output, assembly):
    import json
    import subprocess
    inspector = output / "metadata-inspector"
    inspector.mkdir(parents=True, exist_ok=True)
    (inspector / "Inspector.csproj").write_text('''<Project Sdk="Microsoft.NET.Sdk">
<PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework>
<EnableDefaultCompileItems>false</EnableDefaultCompileItems></PropertyGroup>
<ItemGroup><Compile Include="Program.cs" /></ItemGroup></Project>''')
    (inspector / "Program.cs").write_text('''using System;
using System.IO;
using System.Linq;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Text.Json;
using var input = File.OpenRead(args[0]);
using var pe = new PEReader(input);
if (!pe.HasMetadata) throw new Exception("candidate has no managed metadata");
var metadata = pe.GetMetadataReader();
var types = metadata.TypeDefinitions.Select(h => metadata.GetString(metadata.GetTypeDefinition(h).Name))
    .Where(n => n != "<Module>").ToArray();
if (types.Length == 0) throw new Exception("candidate defines no original types");
var references = metadata.AssemblyReferences.Select(h => metadata.GetString(metadata.GetAssemblyReference(h).Name)).ToArray();
Console.WriteLine(JsonSerializer.Serialize(new { assembly = metadata.GetString(metadata.GetAssemblyDefinition().Name), types, references }));
''')
    dotnet = str(root / ".work/tools/dotnet/dotnet")
    artifacts = output / "metadata-artifacts"
    subprocess.run([dotnet, "build", str(inspector / "Inspector.csproj"), "--artifacts-path",
                    str(artifacts), "--nologo", "--disable-build-servers", "-m:2"], check=True)
    completed = subprocess.run([dotnet, str(artifacts / "bin/Inspector/debug/Inspector.dll"), str(assembly)],
                               check=True, capture_output=True, text=True)
    return json.loads(completed.stdout)
