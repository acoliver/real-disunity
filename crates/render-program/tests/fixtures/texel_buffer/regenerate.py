#!/usr/bin/env python3
"""Regenerate owned fixtures; CI reads the committed binaries without DXC.

DXC: libdxcompiler.dylib: 1.9(1-01b62ad4)(1.9.0.1).
Run: python3 regenerate.py /path/to/dxc
Every exact compiler command is printed. Narrow-format fixtures below patch only
OpTypeImage's format, since DXC chooses a 32-bit format for Buffer<float>.
"""
from pathlib import Path
import shlex
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
DXC = sys.argv[1] if len(sys.argv) > 1 else "/Volumes/XS1000/tools/dxc/build/bin/dxc"
COMMON = ["-spirv", "-HV", "2018",
          "-fvk-use-gl-layout", "-fvk-u-shift", "1", "0", "-E", "main"]


def compile_fixture(name, source, profile="cs_6_0", defines=(), target="vulkan1.1"):
    cmd = [DXC, *COMMON, f"-fspv-target-env={target}", "-T", profile, *defines, str(ROOT / source),
           "-Fo", str(ROOT / f"{name}.spv")]
    print(shlex.join(cmd), flush=True)
    subprocess.run(cmd, check=True)


def patch_format(name, source, image_format):
    data = (ROOT / f"{source}.spv").read_bytes()
    words = list(struct.unpack(f"<{len(data) // 4}I", data))
    at = 5
    count = 0
    while at < len(words):
        size, opcode = words[at] >> 16, words[at] & 0xffff
        if opcode == 25 and words[at + 3] == 5:
            words[at + 8] = image_format
            count += 1
        at += size
    assert count == 1
    (ROOT / f"{name}.spv").write_bytes(struct.pack(f"<{len(words)}I", *words))


def expose_fetch(name):
    """Replace the HLSL padding construct with the image fetch's full vec4."""
    path = ROOT / f"{name}.spv"
    data = path.read_bytes()
    words = list(struct.unpack(f"<{len(data) // 4}I", data))
    output, fetch, count = words[:5], None, 0
    at = 5
    while at < len(words):
        size, opcode = words[at] >> 16, words[at] & 0xffff
        instruction = words[at:at + size]
        if opcode == 95:  # OpImageFetch
            assert fetch is None
            fetch = instruction[1:3]  # Result type and result ID.
        if opcode == 80 and fetch is not None and instruction[1] == fetch[0]:
            instruction = [(4 << 16) | 83, instruction[1], instruction[2], fetch[1]]
            count += 1  # OpCopyObject of the fetched vec4, not the image object.
        output.extend(instruction)
        at += size
    assert count == 1
    path.write_bytes(struct.pack(f"<{len(output)}I", *output))


compile_fixture("palette", "palette.hlsl", "vs_6_0")
for element in ("float", "float2", "float4", "uint", "uint2", "uint4", "int", "int2", "int4"):
    compile_fixture(element, "typed.hlsl", defines=[f"-DELEMENT={element}"])
compile_fixture("write", "refused.hlsl", defines=["-DWRITE=1"])
compile_fixture("query", "refused.hlsl")
patch_format("r16f", "float", 9)
patch_format("rgba8", "float4", 4)
patch_format("unknown", "float", 0)
compile_fixture("multiple", "multiple.hlsl")
compile_fixture("float-vk10", "typed.hlsl", target="vulkan1.0")
compile_fixture("float-vk12", "typed.hlsl", target="vulkan1.2")
for base in ("float", "uint", "int"):
    for components in (1, 2):
        element = base if components == 1 else f"{base}2"
        name = f"{element}-expanded"
        compile_fixture(name, "expanded.hlsl", defines=[f"-DELEMENT={element}",
                        f"-DVECTOR={base}4", f"-DCOMPONENTS={components}"])
        expose_fetch(name)
