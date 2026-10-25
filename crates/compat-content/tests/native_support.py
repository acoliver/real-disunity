"""Build and exercise the public materialization transport with pinned originals."""
import ctypes
from functools import cache
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / "crates/compat-content"
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target/content"))
if not TARGET.is_absolute():
    TARGET = ROOT / TARGET


@cache
def build():
    environment = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    for arguments in [
        ["-p", "worldrunner-original-games", "-p", "worldrunner-disunity-ffi", "--lib", "--bins"],
        ["-p", "worldrunner-compat-content", "--examples"],
    ]:
        subprocess.run(["cargo", "+1.97.0", "build", "--offline", "--jobs", "2", *arguments],
                       cwd=ROOT, env=environment, check=True, timeout=600)


def example(name):
    return str(TARGET / "debug/examples" / name)


def import_fresh(game, output, overrides=()):
    original = subprocess.run([str(TARGET / "debug/original-games"), "path", game],
                              cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip()
    scene = {"snake": "GameScene", "breakout": "SampleScene", "pong": "Main"}[game]
    sidecar = json.loads((BASE / f"overlays/{game}.json").read_text())
    sidecar["field_overrides"].extend(overrides)
    output.mkdir(parents=True, exist_ok=True)
    overlay = output / "overlay.json"
    overlay.write_text(json.dumps(sidecar))
    command = [example("import"), original, f"Assets/Scenes/{scene}.unity",
               str(BASE / f"profiles/{game}-source.json"), str(overlay)]
    imported = subprocess.run(command, check=True, capture_output=True, text=True, cwd=ROOT)
    (output / "graph.json").write_text(imported.stdout)
    graph = json.loads(imported.stdout)
    for path, expected in graph["source_hashes"].items():
        assert hashlib.sha256((Path(original) / path).read_bytes()).hexdigest() == expected, path
    return graph


def materialize(native, game, output):
    result = output / "materialized.json"
    command = [example("materialize"), str(output / "graph.json"),
               str(BASE / f"profiles/{game}-admission.json"), str(result)]
    with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, text=True, cwd=ROOT) as process:
        try:
            for line in process.stdout:
                try:
                    response = native.request(json.loads(line))
                except RuntimeError as error:
                    response = {"$transport_error": str(error)}
                process.stdin.write(json.dumps(response) + "\n")
                process.stdin.flush()
            process.stdin.close()
            error = process.stderr.read()
            if process.wait(timeout=30) != 0:
                raise RuntimeError(error)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=30)
            for stream in (process.stdin, process.stdout, process.stderr):
                if not stream.closed:
                    stream.close()
    return json.loads(result.read_text())


class NativeResult(ctypes.Structure):
    _fields_ = [("code", ctypes.c_int32), ("detail", ctypes.c_uint64)]


class Native:
    def __init__(self):
        self.path = TARGET / "debug/libworldrunner_disunity_ffi.so"
        self.library = ctypes.CDLL(str(self.path))
        types = {
            "disunity_world_create": [ctypes.POINTER(ctypes.c_uint64)],
            "disunity_world_destroy": [ctypes.c_uint64],
            "disunity_world_request": [ctypes.c_uint64, ctypes.c_void_p, ctypes.c_size_t,
                                       ctypes.POINTER(ctypes.c_uint64)],
            "disunity_response_copy": [ctypes.c_uint64, ctypes.c_void_p, ctypes.c_size_t,
                                       ctypes.POINTER(ctypes.c_size_t)],
            "disunity_response_free": [ctypes.c_uint64],
            "disunity_store_statistics": [ctypes.POINTER(ctypes.c_uint64), ctypes.POINTER(ctypes.c_uint64)],
        }
        for name, arguments in types.items():
            function = getattr(self.library, name)
            function.argtypes = arguments
            function.restype = NativeResult
        world = ctypes.c_uint64()
        self.check(self.library.disunity_world_create(ctypes.byref(world)))
        self.world = world.value

    @staticmethod
    def check(result):
        if result.code != 0:
            raise RuntimeError(f"native transport failed: code={result.code}, detail={result.detail}")

    def request(self, value):
        source = json.dumps(value, separators=(",", ":")).encode()
        response = ctypes.c_uint64()
        self.check(self.library.disunity_world_request(
            self.world, source, len(source), ctypes.byref(response)))
        try:
            size = ctypes.c_size_t()
            probe = self.library.disunity_response_copy(response.value, None, 0, ctypes.byref(size))
            if probe.code != 2 or size.value == 0:
                raise RuntimeError(f"unexpected native response probe: {probe.code}")
            buffer = ctypes.create_string_buffer(size.value)
            self.check(self.library.disunity_response_copy(
                response.value, buffer, size.value, ctypes.byref(size)))
            value = json.loads(buffer.raw[:size.value])
            if value.get("ok") is not True:
                raise RuntimeError(f"native request rejected: {value}")
            return value["data"]
        finally:
            self.check(self.library.disunity_response_free(response.value))

    def statistics(self):
        live, allocated = ctypes.c_uint64(), ctypes.c_uint64()
        self.check(self.library.disunity_store_statistics(ctypes.byref(live), ctypes.byref(allocated)))
        return {"live_worlds": live.value, "allocated_world_slots": allocated.value}

    def close(self):
        self.check(self.library.disunity_world_destroy(self.world))


