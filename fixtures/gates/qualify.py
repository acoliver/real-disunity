#!/usr/bin/env python3
"""Compile candidate original scripts and test them through the configured worker.

The refresh supervisor owns the process group, deadline and capped logs. Every
build product and generated project stays in COMPAT_GATE_OUTPUT under .work.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
from xml.sax.saxutils import escape
from qualification import digest, inspect_assembly, select_sources, verify_sources
from candidate import content_receipt, local_identity, verify_content, verify_identity


def main():
    name, stage = sys.argv[1:]
    root = Path(os.environ["COMPAT_ROOT"]).resolve()
    source = Path(os.environ["COMPAT_SOURCE_ROOT"]).resolve()
    output = Path(os.environ["COMPAT_GATE_OUTPUT"]).resolve()
    output.relative_to(root / ".work")
    source.relative_to(root / ".work")
    fixture = json.loads((root / "fixtures" / (name + ".json")).read_text())
    selection = json.loads((root / "fixtures/gates/sources.json").read_text())[name]
    executable = Path(os.environ.get("COMPAT_CONTENT_RECEIPT", str(root / ".work/target/debug/examples/receipt")))
    identity = local_identity(source)
    content = content_receipt(source, fixture, output, executable)
    verify_identity(source, identity)
    hashes = select_sources(source, selection)
    if stage == "compile":
        project_dir = output / "compile"
        project_dir.mkdir(parents=True, exist_ok=True)
        scripts = [source / relative for relative in hashes]
        print(json.dumps({"selected_count": len(scripts), "source_hashes": hashes}), flush=True)
        includes = "\n".join('<Compile Include="' + escape(str(p), {'"': '&quot;'}) + '" />' for p in scripts)
        facade = escape(str(root / "csharp/UnityCompat/UnityCompat.csproj"), {'"': '&quot;'})
        project = project_dir / "Candidate.csproj"
        project.write_text(f'''<Project Sdk="Microsoft.NET.Sdk">
<PropertyGroup><TargetFramework>net8.0</TargetFramework><Nullable>enable</Nullable>
<ImplicitUsings>disable</ImplicitUsings><EnableDefaultCompileItems>false</EnableDefaultCompileItems>
<AssemblyName>Candidate_{name}</AssemblyName></PropertyGroup>
<ItemGroup><ProjectReference Include="{facade}" />{includes}</ItemGroup></Project>''')
        artifacts = output / "dotnet-artifacts"
        command = [str(root / ".work/tools/dotnet/dotnet"), "build", str(project),
                   "--artifacts-path", str(artifacts), "--nologo", "--disable-build-servers", "-m:2"]
        completed = subprocess.run(command, check=False)
        if completed.returncode == 0:
            assembly = artifacts / "bin" / "Candidate" / "debug" / ("Candidate_" + name + ".dll")
            if not assembly.is_file():
                raise RuntimeError("compile succeeded without expected candidate assembly")
            metadata = inspect_assembly(root, output, assembly)
            verify_sources(source, selection, hashes)
            verify_content(source, fixture, output, executable, content)
            verify_identity(source, identity)
            (output / "candidate-assembly.json").write_text(json.dumps({
                "assembly": str(assembly), "assembly_sha256": digest(assembly),
                "source_root": str(source), "source_count": len(hashes),
                "source_hashes": hashes, "metadata": metadata,
                "content_receipt": content, "local_identity": identity,
                "excluded_scripts": sorted(str(p.relative_to(source)) for p in (source / "Assets").rglob("*.cs") if str(p.relative_to(source)) not in hashes)
            }, indent=2))
        return completed.returncode
    if stage == "test":
        if not fixture.get("worker"):
            print("blocked: facade-backed worker is not configured", file=sys.stderr)
            return 2
        proof = json.loads((output / "candidate-assembly.json").read_text())
        assembly = proof["assembly"]
        verify_content(source, fixture, output, executable, proof["content_receipt"])
        verify_identity(source, proof["local_identity"])
        if proof["source_root"] != str(source) or proof["source_count"] != len(hashes):
            raise RuntimeError("compile proof source identity/count mismatch")
        verify_sources(source, selection, proof["source_hashes"])
        if digest(Path(assembly)) != proof["assembly_sha256"]:
            raise RuntimeError("compiled assembly changed since qualification")
        if inspect_assembly(root, output, Path(assembly)) != proof["metadata"]:
            raise RuntimeError("compiled assembly metadata changed")
        fixture["name"] = name + "-candidate"
        fixture["source_root"] = str(source)
        fixture["metadata"]["assembly"] = assembly
        fixture["metadata"]["assembly_qualification"] = str(output / "candidate-assembly.json")
        fixture["metadata"]["script_count"] = len(hashes)
        fixture["refresh"] = None
        candidate_fixture = output / "candidate-fixture.json"
        candidate_fixture.write_text(json.dumps(fixture, indent=2))
        harness = root / ".work/target/debug/compat-kit"
        # Every declared profile is a gate. The normal harness evaluates actual
        # observations and validates native/managed/readback teardown evidence.
        for profile in fixture["profiles"]:
            verify_content(source, fixture, output, executable, proof["content_receipt"])
            verify_identity(source, proof["local_identity"])
            completed = subprocess.run([str(harness), "run", str(candidate_fixture), "--profile", profile], check=False)
            if completed.returncode:
                return completed.returncode
            verify_sources(source, selection, proof["source_hashes"])
            verify_content(source, fixture, output, executable, proof["content_receipt"])
            verify_identity(source, proof["local_identity"])
            if digest(Path(assembly)) != proof["assembly_sha256"]:
                raise RuntimeError("compiled assembly changed during runtime qualification")
        return 0
    raise RuntimeError("unknown qualification stage: " + stage)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        print("qualification error: " + str(error), file=sys.stderr)
        sys.exit(2)
