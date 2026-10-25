"""Plan WASD from real native observations, then replay through compat-kit.

This module contains grid navigation and evidence validation. All gameplay state
and transitions come from the unchanged source scripts running in compat-player.
"""

import argparse
from collections import deque
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import struct
import subprocess
import sys
import time
import uuid
import zlib

FOOD = "managed:FoodSpawner:CurrentFoodPos"
WORLD = "world.snapshot"
HEAD = "object:SnakeSegment(Clone)"
LABEL = "original-source/authored-missing-sprite"
OFFSETS = {"W": (0, 1), "S": (0, -1), "A": (-1, 0), "D": (1, 0)}
OPPOSITE = {"W": "S", "S": "W", "A": "D", "D": "A"}
ROOT = Path(__file__).resolve().parents[2]

sys.path.insert(0, str(ROOT / "tools"))
from original_games import checkout


class JourneyError(RuntimeError):
    pass


class ProviderBlocked(JourneyError):
    pass


def require(condition, message):
    if not condition:
        raise JourneyError(message)


def read_json(path):
    return json.loads(path.read_text())


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def work_path(root, path):
    path = path.resolve()
    require(path.is_relative_to((root / ".work").resolve()), f"output must be inside .work: {path}")
    return path


def neighbor(point, key, size, wrap):
    dx, dy = OFFSETS[key]
    x, y = point[0] + dx, point[1] + dy
    return (x % size[0], y % size[1]) if wrap else (x, y)


def route_to(head, target, body, direction, size, wrap):
    """BFS through observed free cells. No prediction of body motion or food."""
    blocked = set(body)
    blocked.discard(head)
    queue = deque([(head, direction, [])])
    visited = {(head, direction)}
    while queue:
        point, heading, route = queue.popleft()
        if point == target:
            return route
        for key in OFFSETS:
            if key == OPPOSITE[heading]:
                continue
            nxt = neighbor(point, key, size, wrap)
            if not (0 <= nxt[0] < size[0] and 0 <= nxt[1] < size[1]) or nxt in blocked:
                continue
            state = (nxt, key)
            if state not in visited:
                visited.add(state)
                queue.append((nxt, key, route + [key]))
    raise JourneyError(f"no safe static-grid route from {head} toward {target}")


def observation(report, selector, frame):
    values = [o["value"] for o in report.get("observations", [])
              if o["selector"] == selector and o["frame"] == frame]
    require(len(values) == 1, f"required public observation {selector} at frame {frame}: found {len(values)}")
    return values[0]


def clone_rows(snapshot, segment_name="SnakeSegment"):
    return [row for row in snapshot["objects"] if row["name"] in (segment_name, segment_name + "(Clone)")
            and row.get("active_in_hierarchy", row.get("active", False))]


def visible_body(snapshot, cell_size, pool_order=None, segment_name="SnakeSegment"):
    rows = clone_rows(snapshot, segment_name)
    if pool_order is not None:
        ranks = {handle: i for i, handle in enumerate(pool_order)}
        require(all(row["handle"] in ranks for row in rows), "unrecognized native clone identity")
        rows.sort(key=lambda row: ranks[row["handle"]])
    result = []
    for row in rows:
        position = row["transform"]["position"]
        grid = (position[0] / cell_size, position[1] / cell_size)
        require(all(abs(axis - round(axis)) < 1e-4 for axis in grid), f"off-grid visible segment: {position}")
        result.append(tuple(round(axis) for axis in grid))
    return result


def public_point(value):
    require(isinstance(value, dict) and isinstance(value.get("x"), int) and isinstance(value.get("y"), int),
            f"public CurrentFoodPos must expose integer x/y, received {value!r}")
    return value["x"], value["y"]


def metadata(root):
    return {"scene": "Assets/Scenes/GameScene.unity", "script_count": 8,
            "import_profile": read_json(root / "crates/compat-content/profiles/snake-source.json"),
            "admission_policy": read_json(root / "crates/compat-content/profiles/snake-admission.json"),
            "compatibility_sidecar": read_json(root / "crates/compat-content/overlays/snake.json"),
            "viewport": [960, 720], "evidence_label": LABEL,
            "source_scripts": "original, eight unchanged Assets/Scripts C# files",
            "visual_variant": "authored-missing-sprite", "reset_mechanism": "fixture-reload",
            "native_update_contract": (
                "Each logical tick duration is subdivided into 1/60-second native Updates plus a final remainder. "
                "Logical frame labels retain input/observation/capture boundaries."),
            "qualification": "facade-backed source execution; no Unity compiler/editor certification"}


def action(frame, name, **values):
    return {"frame": frame, "phase": "observe" if name == "observe" else name,
            "action": name, **values}


def native_updates(timeline):
    """Keep logical input/capture frames while bounding each native Update delta."""
    result = []
    for entry in timeline:
        if entry["action"] != "advance" or entry["dt"] == 0:
            result.append(dict(entry))
            continue
        remaining = entry["dt"]
        require(math.isfinite(remaining) and remaining > 0, "advance delta must be finite and nonnegative")
        while remaining > 0:
            dt = min(remaining, 1 / 60)
            result.append({**entry, "dt": dt})
            remaining -= dt
    return result


def observe_actions(frame, food=True):
    result = [action(frame, "observe", selector=WORLD)]
    if food:
        result.append(action(frame, "observe", selector=FOOD))
    return result


def command(root, executable, arguments, folder, timeout=180):
    """Run a selected executable with a bounded process-group lifetime."""
    stdout, stderr = folder / "stdout.log", folder / "stderr.log"
    with stdout.open("wb") as out, stderr.open("wb") as err:
        process = subprocess.Popen([str(executable), *map(str, arguments)],
                                   cwd=root, env=os.environ.copy(), stdout=out, stderr=err, start_new_session=True)
        try:
            status = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired as error:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise ProviderBlocked(f"provider timeout after {timeout}s; {folder}") from error
    require(stdout.stat().st_size + stderr.stat().st_size <= 16_777_216, f"provider log limit exceeded: {folder}")
    return status


class Worker:
    def __init__(self, root, output, executable, configuration, source, harness):
        self.root, self.output, self.executable = root, output, executable
        self.configuration = configuration
        self.source, self.harness = source, harness
        self.calls = 0
        self.last_folder = None

    def run(self, timeline, profile, captures=False):
        if not self.executable.is_file():
            raise ProviderBlocked(f"worker unavailable: {self.executable}")
        require(self.calls < 100, "prefix run safety bound reached (100)")
        self.calls += 1
        folder = work_path(self.root, self.output / f"prefix-{self.calls:03d}")
        folder.mkdir(parents=True)
        self.last_folder = folder
        request = {"schema_version": 1, "run_id": folder.name, "fixture": "snake-journey",
                   "profile": profile, "desktop": False, "seed": 17, "timeline": timeline,
                   "assertions": [], "source_root": str(self.source),
                   "artifact_root": str(folder), "metadata": self.configuration}
        write_json(folder / "request.json", request)
        old = {name: os.environ.get(name) for name in ("COMPAT_ROOT", "COMPAT_REQUEST", "COMPAT_RESPONSE", "COMPAT_ARTIFACTS")}
        os.environ.update(COMPAT_ROOT=str(self.root), COMPAT_REQUEST=str(folder / "request.json"),
                          COMPAT_RESPONSE=str(folder / "response.json"), COMPAT_ARTIFACTS=str(folder))
        try:
            status = command(self.root, self.executable, [], folder)
        finally:
            for name, value in old.items():
                if value is None:
                    os.environ.pop(name, None)
                else:
                    os.environ[name] = value
        if status or not (folder / "response.json").is_file():
            raise ProviderBlocked(f"worker failed ({status}); see {folder}/stderr.log")
        report = read_json(folder / "response.json")
        diagnostics = [d["message"] for d in report.get("diagnostics", []) if d["status"] != "pass"]
        if diagnostics:
            raise ProviderBlocked(f"worker provider diagnostic: {'; '.join(diagnostics)}; {folder}")
        require(report["evidence_label"] == LABEL, "provider changed evidence origin")
        teardown = report["teardown"]
        require(teardown == {"completed": True, "native_worlds": 0, "managed_roots": 0, "pending_readbacks": 0},
                f"provider teardown incomplete: {teardown}; {folder}")
        return report


def decode_png(path):
    """Decode canonical non-interlaced RGB/RGBA 8-bit GPU PNGs using stdlib."""
    data = path.read_bytes()
    require(len(data) <= 67_108_864 and data.startswith(b"\x89PNG\r\n\x1a\n"), f"invalid PNG: {path}")
    cursor, compressed, header, ended = 8, bytearray(), None, False
    while cursor < len(data):
        require(cursor + 12 <= len(data), "truncated PNG chunk")
        length = struct.unpack_from(">I", data, cursor)[0]
        kind = data[cursor + 4:cursor + 8]
        require(cursor + 12 + length <= len(data), "truncated PNG payload")
        payload = data[cursor + 8:cursor + 8 + length]
        crc = struct.unpack_from(">I", data, cursor + 8 + length)[0]
        require(zlib.crc32(kind + payload) == crc, "PNG checksum mismatch")
        if kind == b"IHDR":
            require(length == 13 and header is None, "invalid PNG header")
            header = struct.unpack(">IIBBBBB", payload)
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            require(length == 0, "invalid PNG end")
            ended = True
            cursor += length + 12
            break
        cursor += length + 12
    require(header is not None and ended and cursor == len(data), "incomplete PNG")
    width, height, depth, color, compression, filtering, interlace = header
    require(0 < width <= 8192 and 0 < height <= 8192 and depth == 8 and color in (2, 6)
            and compression == filtering == interlace == 0, f"unsupported canonical PNG format: {header}")
    channels = 4 if color == 6 else 3
    stride = width * channels
    expected = height * (stride + 1)
    require(expected <= 67_108_864, "PNG decoded byte limit exceeded")
    inflater = zlib.decompressobj()
    try:
        raw = inflater.decompress(compressed, expected + 1)
    except zlib.error as error:
        raise JourneyError(f"PNG decompression failed: {error}") from error
    require(len(raw) == expected and inflater.eof and not inflater.unused_data, "PNG scanline size mismatch")
    return width, height, decode_scanlines(raw, width, height, channels)


def decode_scanlines(raw, width, height, channels):
    stride = width * channels
    previous, pixels = bytearray(stride), bytearray()
    for y in range(height):
        start = y * (stride + 1)
        method = raw[start]
        require(method in range(5), "invalid PNG scanline filter")
        row = bytearray(raw[start + 1:start + 1 + stride])
        for x in range(stride):
            left, up = row[x - channels] if x >= channels else 0, previous[x]
            corner = previous[x - channels] if x >= channels else 0
            if method == 1:
                row[x] = (row[x] + left) & 255
            elif method == 2:
                row[x] = (row[x] + up) & 255
            elif method == 3:
                row[x] = (row[x] + (left + up) // 2) & 255
            elif method == 4:
                p = left + up - corner
                distances = [abs(p - left), abs(p - up), abs(p - corner)]
                prediction = [left, up, corner][distances.index(min(distances))]
                row[x] = (row[x] + prediction) & 255
        if channels == 4:
            pixels.extend(row)
        else:
            for x in range(0, stride, 3):
                pixels.extend(row[x:x + 3])
                pixels.append(255)
        previous = row
    return bytes(pixels)


def validate_capture_pixels(decoded):
    require(bool(decoded), "no canonical GPU captures")
    for width, height, pixels in decoded:
        require(len(pixels) == width * height * 4, "invalid decoded capture size")
        require(any(pixels[i:i + 4] != pixels[:4] for i in range(0, len(pixels), 4)),
                "uniform/blank canonical renderer capture")
    if len(decoded) > 1:
        require(len({hashlib.sha256(pixels).digest() for _, _, pixels in decoded}) > 1,
                "canonical capture pixels unchanged across movement")


def config_from_graph(graph):
    configs = [c["script"]["fields"] for c in graph["components"] if (c.get("script") or {}).get("type_name") == "GameConfig"]
    views = [c["script"]["fields"] for c in graph["components"] if (c.get("script") or {}).get("type_name") == "SnakeView"]
    require(len(configs) == len(views) == 1, "expected one bound authored GameConfig and SnakeView")
    cfg = configs[0]
    spawners = [c["script"]["fields"] for c in graph["components"]
                if (c.get("script") or {}).get("type_name") == "FoodSpawner"]
    require(len(spawners) == 1, "cannot resolve authored FoodSpawner")
    food_reference = spawners[0]["foodVisual"]["$ref"]
    food_nodes = [node for node in graph["nodes"] if food_reference in node["components"]]
    require(len(food_nodes) == 1, "cannot resolve authored FoodSpawner.foodVisual")
    segment_reference = views[0]["segmentPrefab"]["$ref"]
    segment_nodes = [node for node in graph["nodes"] if segment_reference in node["components"]]
    require(len(segment_nodes) == 1, "cannot resolve authored SnakeView.segmentPrefab")
    size = cfg["gridSize"]
    result = {"size": (size["x"], size["y"]), "wrap": bool(cfg["wrapAround"]),
              "rate": float(cfg["ticksPerSecond"]), "increase": float(cfg["speedIncreasePerFood"]),
              "maximum": float(cfg["maxTicksPerSecond"]), "cell_size": float(views[0]["cellSize"]), "segment_name": segment_nodes[0]["name"], "food_name": food_nodes[0]["name"]}
    require(result["size"][0] > 5 and result["size"][1] > 5 and result["cell_size"] > 0
            and 0 < result["rate"] < result["maximum"] and result["increase"] > 0,
            f"authored config cannot support this journey: {result}")
    require(result["wrap"], "source-authored wrapAround=false: wrapping journey requires a labeled config-sidecar variant; no silent overrides")
    return result


def pointer_escape(value):
    return value.replace("~", "~0").replace("/", "~1")


def equal(selector, frame, value, pointer=""):
    return {"op": "equal", "selector": selector, "frame": frame, "phase": "observe", "pointer": pointer, "value": value}


def position_assertions(frame, point, cell_size, index):
    return [{"op": "tolerance", "selector": WORLD, "pointer": f"/objects/{index}/transform/position/{axis}",
             "frame": frame, "phase": "observe", "value": value * cell_size, "tolerance": 0.0001}
            for axis, value in enumerate(point)]


def check_transition(before, after, expected_head, pending_growth, stationary=False):
    require(bool(after["body"]), "no native-visible snake head")
    require(after["body"][0] == expected_head,
            f"head did not follow source tick/input cadence: expected {expected_head}, got {after['body'][0]}")
    if stationary:
        require(after["body"] == before["body"] and after["food"] == before["food"],
                "stationary/collision observation moved body or respawned food")
        return False
    require(len(after["body"]) == len(before["body"]) + int(pending_growth),
            f"deferred growth mismatch: {len(before['body'])} -> {len(after['body'])}, pending={pending_growth}")
    ate = expected_head == before["food"]
    require((after["food"] != before["food"]) == ate,
            "food respawn must coincide with visible head reaching previous public CurrentFoodPos")
    require(after["food"] not in after["body"], "source food spawned in observed occupied cell")
    return ate


def state_at(report, frame, cfg, pool):
    snapshot = observation(report, WORLD, frame)
    rows = clone_rows(snapshot, cfg["segment_name"])
    for row in rows:
        if row["handle"] not in pool:
            pool.append(row["handle"])
    body = visible_body(snapshot, cfg["cell_size"], pool, cfg["segment_name"])
    food = public_point(observation(report, FOOD, frame))
    food_rows = [row for row in snapshot["objects"] if row["name"] == cfg["food_name"]
                 and row.get("active_in_hierarchy", row.get("active", False))]
    require(len(food_rows) == 1, "authored foodVisual must have one active native instance")
    require(all(abs(food_rows[0]["transform"]["position"][i] - food[i]) < 1e-4 for i in range(2)),
            "public CurrentFoodPos differs from native foodVisual world pose")
    indices = [next(i for i, row in enumerate(snapshot["objects"]) if row["handle"] == handle)
               for handle in pool if any(row["handle"] == handle for row in rows)]
    return {"body": body, "food": food, "indices": indices}


def game_over_lines(path):
    return sum(line.strip() == "Game Over" for line in path.read_text(errors="replace").splitlines())


class Planner:
    def __init__(self, runner, profile, cfg, summary):
        self.runner, self.profile, self.cfg, self.summary = runner, profile, cfg, summary
        self.frame, self.direction, self.eaten = 1, "D", 0
        self.timeline = [action(0, "load"), action(1, "advance", dt=1 / cfg["rate"] + 0.000001),
                         *observe_actions(1)]
        self.steps = []
        self.states = {}
        self.pending_growth = False
        self.stationary_ticks = 0
        self.checkpoints = {"initial": 1}
        self.refresh()
        expected = (cfg["size"][0] // 2 + 1, cfg["size"][1] // 2)
        require(self.state["body"] == [expected], f"initial native head does not match authored spawn/tick: {self.state}")
        self.summary["initial_food"] = self.state["food"]
        self.fact("initial source tick and native clone", frame=1, head=expected)

    @property
    def state(self):
        return self.states[self.frame]

    @property
    def interval(self):
        return 1 / min(self.cfg["rate"] + self.eaten * self.cfg["increase"], self.cfg["maximum"])

    def fact(self, name, **detail):
        self.summary["behavioral_checks"].append({"check": name, **detail})
        write_json(self.runner.output / "progress.json", self.summary)
        print(json.dumps({"progress": name, **detail}), flush=True)

    def refresh(self):
        report = self.runner.run(self.timeline, self.profile)
        actual_cfg = config_from_graph(read_json(self.runner.last_folder / "imported-scene.json"))
        require(actual_cfg == self.cfg, f"authored config changed from planning input: {self.cfg} -> {actual_cfg}")
        pool, states = [], {}
        reset_frames = {step["frame"] for step in self.steps if step["kind"] == "reload"}
        for entry in report["observations"]:
            if entry["selector"] != WORLD:
                continue
            frame = entry["frame"]
            if frame in reset_frames:
                pool = []
            states[frame] = state_at(report, frame, self.cfg, pool)
            if frame in self.states:
                require(states[frame] == self.states[frame], f"seeded prefix replay diverged at frame {frame}")
        for step in self.steps:
            frame = step["frame"]
            if frame in self.states:
                continue
            after = states[frame]
            before = states[step["before"]]
            if step["kind"] == "reload":
                require(after["body"] == [step["head"]] and after["food"] == states[1]["food"],
                        "fixture reload did not recreate original head and seeded food")
                self.eaten, self.pending_growth = 0, False
            else:
                ate = check_transition(before, after, step["head"], self.pending_growth,
                                       stationary=step["kind"] == "stationary")
                self.pending_growth = ate
                self.eaten += int(ate)
                if step["kind"] == "stationary":
                    self.stationary_ticks += 1
        self.states = states
        require(game_over_lines(self.runner.last_folder / "stdout.log") == self.stationary_ticks,
                f"source Game Over stdout count must match observed collision/dead ticks ({self.stationary_ticks})")
        write_json(self.runner.output / "observed-trace.json", {str(k): v for k, v in states.items()})

    def append(self, key, expected_head, kind="move", dt=None):
        require(self.frame < 600, "journey frame safety bound (600) exceeded")
        before = self.frame
        self.frame += 1
        # Release before a down edge, even if the same key was used last tick.
        self.timeline.extend([action(self.frame, "input", keys=[], axes={}),
                              action(self.frame, "input", keys=[key], axes={}),
                              action(self.frame, "advance", dt=self.interval + 0.000001 if dt is None else dt),
                              *observe_actions(self.frame)])
        self.steps.append({"frame": self.frame, "before": before, "head": expected_head, "kind": kind})
        if key != OPPOSITE[self.direction]:
            self.direction = key

    def route(self, keys, reason):
        point = self.state["body"][0]
        start, heading = self.frame, self.direction
        for key in keys:
            effective = heading if key == OPPOSITE[heading] else key
            point = neighbor(point, effective, self.cfg["size"], self.cfg["wrap"])
            self.append(key, point)
            heading = effective
        self.refresh()
        self.fact(reason, before=start, after=self.frame, head=self.state["body"][0], ticks=len(keys))

    def safe_step(self):
        head = self.state["body"][0]
        for key in OFFSETS:
            target = neighbor(head, key, self.cfg["size"], self.cfg["wrap"])
            if key != OPPOSITE[self.direction] and target not in self.state["body"] and target != self.state["food"]:
                return key
        raise JourneyError("no free observed neighbor for deferred growth proof")

    def turns_reversal_wrap(self):
        self.route(["A"], "opposite WASD reversal ignored")
        self.route(["W", "A", "S", "D"], "four orthogonal WASD turns")
        target = (self.cfg["size"][0] - 1, self.state["body"][0][1])
        blocked = [*self.state["body"], self.state["food"]]
        require(target != self.state["food"], "wrap waypoint occupied by food; choose another observed edge row")
        route = route_to(self.state["body"][0], target, blocked, self.direction, self.cfg["size"], True)
        self.route(route, "navigate to authored right boundary")
        require(self.direction != "A", "wrap approach ended opposite required boundary crossing")
        self.route(["D"], "authored wrapAround crosses x=max to x=0")
        require(self.state["body"][0][0] == 0, "authored wrap did not cross to x=0")
        self.checkpoints["wrap-x"] = self.frame
        target = (self.state["body"][0][0], self.cfg["size"][1] - 1)
        require(target != self.state["food"], "vertical wrap waypoint occupied by observed food")
        route = route_to(self.state["body"][0], target, [*self.state["body"], self.state["food"]],
                         self.direction, self.cfg["size"], True)
        self.route(route, "navigate to authored upper boundary")
        require(self.direction != "S", "vertical wrap approach ended opposite required crossing")
        self.route(["W"], "authored wrapAround crosses y=max to y=0")
        require(self.state["body"][0][1] == 0, "authored wrap did not cross to y=0")
        self.checkpoints["wrap-y"] = self.frame

    def food_growth_speed(self):
        for _ in range(8):
            if self.eaten >= 5 and len(self.state["body"]) >= 6:
                return
            before, old_length, old_food = self.frame, len(self.state["body"]), self.state["food"]
            keys = route_to(self.state["body"][0], old_food, self.state["body"], self.direction,
                            self.cfg["size"], self.cfg["wrap"])
            require(bool(keys), "public food already occupies observed head")
            self.route(keys, "head reaches public food and source respawns it")
            require(self.state["body"][0] == old_food and len(self.state["body"]) == old_length,
                    "food must defer visible growth until following tick")
            eating = self.frame
            faster_dt = self.interval + 0.000001
            require(faster_dt < 1 / self.cfg["rate"], "authored speed increment does not produce shorter interval")
            self.route([self.safe_step()], "next native tick grows one segment at increased speed")
            require(len(self.state["body"]) == old_length + 1, "growth is not visible on next tick")
            self.fact("food/growth/speed observed", approach=before, eating=eating, growth=self.frame,
                      length=len(self.state["body"]), shortened_update_dt=faster_dt,
                      original_interval=1 / self.cfg["rate"])
            self.checkpoints[f"food-{self.eaten}"] = eating
            self.checkpoints[f"growth-{self.eaten}"] = self.frame
        raise JourneyError("eight real food routes did not produce five foods and six visible segments")

    def collision_and_reload(self):
        length = len(self.state["body"])
        head, blocked = self.state["body"][0], {*self.state["body"][1:], self.state["food"]}
        straight = None
        for key in OFFSETS:
            if key == OPPOSITE[self.direction]:
                continue
            line, cursor = [], head
            for _ in range(length + 1):
                cursor = neighbor(cursor, key, self.cfg["size"], True)
                line.append(cursor)
            if all(point not in blocked for point in line):
                straight = key
                break
        require(straight is not None, "no observed free straight corridor for source self-collision setup")
        self.route([straight] * (length + 1), "straighten observed body through ordinary WASD")
        perpendicular = ("A", "D") if straight in ("W", "S") else ("W", "S")
        first = None
        for key in perpendicular:
            one = neighbor(self.state["body"][0], key, self.cfg["size"], True)
            two = neighbor(one, OPPOSITE[straight], self.cfg["size"], True)
            if one not in self.state["body"] and two not in self.state["body"] and self.state["food"] not in (one, two):
                first = key
                break
        require(first is not None, "no free observed two-turn collision approach")
        self.route([first, OPPOSITE[straight]], "approach own visible body without state injection")
        final = OPPOSITE[first]
        target = neighbor(self.state["body"][0], final, self.cfg["size"], True)
        require(target in self.state["body"][1:-1], "planned collision target must be observed occupied non-tail cell")
        self.append(final, self.state["body"][0], kind="stationary")
        self.refresh()
        self.checkpoints["self-collision"] = self.frame
        self.fact("self-collision emits source Game Over and freezes native body", frame=self.frame, target=target)
        for key in (straight, first):
            self.append(key, self.state["body"][0], kind="stationary")
            self.refresh()
        self.fact("post-death WASD cannot move visible head", frame=self.frame, game_over_lines=self.stationary_ticks)
        before = self.frame
        self.frame += 1
        self.timeline.append(action(self.frame, "reload"))
        self.frame += 1
        self.timeline.extend([action(self.frame, "advance", dt=1 / self.cfg["rate"] + 0.000001),
                              *observe_actions(self.frame)])
        spawn = (self.cfg["size"][0] // 2 + 1, self.cfg["size"][1] // 2)
        self.steps.append({"frame": self.frame, "before": before, "head": spawn, "kind": "reload"})
        self.direction = "D"
        self.refresh()
        self.checkpoints["fixture-reload"] = self.frame
        self.fact("fixture reload recreates initial source state and seeded food", frame=self.frame)
        self.route(["W"], "renewed WASD control after fixture reload")
        self.checkpoints["renewed-control"] = self.frame

    def fixture(self):
        assertions = []
        for frame, state in self.states.items():
            assertions += position_assertions(frame, state["body"][0], self.cfg["cell_size"], state["indices"][0])
            assertions.append(equal(FOOD, frame, {"x": state["food"][0], "y": state["food"][1]}))
            for index, point in zip(state["indices"], state["body"]):
                assertions.append(equal(WORLD, frame, True, f"/objects/{index}/active_in_hierarchy"))
                for axis in range(2):
                    assertions.append({"op": "tolerance", "selector": WORLD,
                                       "pointer": f"/objects/{index}/transform/position/{axis}",
                                       "frame": frame, "phase": "observe", "value": point[axis] * self.cfg["cell_size"],
                                       "tolerance": 0.0001})
        assertions.append({"op": "event_order", "selector": FOOD, "pointer": "", "events": [
            {"x": state["food"][0], "y": state["food"][1]} for state in self.states.values()]})
        timeline = list(self.timeline)
        for name, frame in self.checkpoints.items():
            timeline.append({**action(frame, "capture", path=f"captures/{name}.png"), "phase": "render"})
            assertions.append({"op": "capture", "frame": frame, "phase": "render", "origin": LABEL})
        timeline.sort(key=lambda entry: entry["frame"])
        timeline.append(action(self.frame + 1, "unload"))
        cfg = dict(self.runner.configuration)
        cfg.update(journey_checkpoints=self.checkpoints, journey_observed_config=self.cfg)
        worker = str(self.runner.executable)
        return {"schema_version": 1, "name": "SnakeJourney", "game": "snake",
                "profiles": ["source-6000.3", "compat-6000.3", "compat-2017.1"], "default_profile": self.profile,
                "evidence_label": LABEL, "worker": {"program": worker, "args": []},
                "seed": 17, "limits": {"deadline_ms": 180000, "log_bytes": 1048576,
                                        "artifact_bytes": 536870912, "capture_count": 32},
                "timeline": native_updates(timeline), "assertions": assertions, "metadata": cfg}


def replay_harness(planner, fixture_path, desktop):
    runner = planner.runner
    folder = runner.output / ("desktop" if desktop else "gpu")
    folder.mkdir()
    executable = runner.harness
    if not executable.is_file():
        raise ProviderBlocked(f"harness unavailable: {executable}")
    arguments = ["run", fixture_path, "--profile", planner.profile]
    if desktop:
        arguments.append("--desktop")
    status = command(runner.root, executable, arguments, folder, timeout=240)
    try:
        record = read_json(folder / "stdout.log")
    except ValueError as error:
        raise ProviderBlocked(f"canonical harness produced no run record: {folder}") from error
    write_json(folder / "record.json", record)
    require(status == 0 and record["status"] == "pass",
            f"canonical {'desktop' if desktop else 'GPU'} run failed: {record.get('messages')}; {folder}/record.json")
    run_dir = work_path(runner.root, runner.root / record["run_dir"])
    report = record["worker"]
    pool, states = [], {}
    reload_frame = planner.checkpoints["fixture-reload"]
    for frame, expected in planner.states.items():
        if frame == reload_frame:
            pool = []
        actual = state_at(report, frame, planner.cfg, pool)
        require(actual == expected, f"canonical replay differs from source-validated prepass at frame {frame}")
        states[frame] = actual
    require(game_over_lines(run_dir / "stdout.log") == planner.stationary_ticks,
            "canonical source Game Over output differs from collision/death assertions")
    captures = [work_path(runner.root, run_dir / capture["path"]) for capture in record["captures"]]
    validate_capture_pixels([decode_png(path) for path in captures])
    return {"status": "pass", "record": str(folder / "record.json"), "run_dir": str(run_dir),
            "captures": [str(path) for path in captures], "assertions": len(record["assertions"]),
            "game_over_lines": planner.stationary_ticks}


def execute_journey(runner, args, summary):
    authored_graph = read_json(args.graph)
    cfg = config_from_graph(authored_graph)
    summary["authored_config"] = cfg
    planner = Planner(runner, args.profile, cfg, summary)
    planner.turns_reversal_wrap()
    planner.food_growth_speed()
    require(planner.eaten >= 5 and len(planner.state["body"]) >= 6,
            "complete journey requires five actual foods and six native-visible segments")
    planner.collision_and_reload()
    fixture_path = runner.output / "fixture.json"
    write_json(fixture_path, planner.fixture())
    summary.update(fixture=str(fixture_path), observed_frames=len(planner.states), prefix_runs=runner.calls,
                   checkpoints=planner.checkpoints, game_over_lines=planner.stationary_ticks)
    summary["gpu"] = replay_harness(planner, fixture_path, False)
    if args.desktop:
        summary["desktop"] = replay_harness(planner, fixture_path, True)
        summary["desktop_status"] = "pass"
        summary["status"] = "pass"
    else:
        summary.update(status="inconclusive", message="GPU journey passed; real desktop replay requires --desktop")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["probe", "run"])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--harness", type=Path, required=True)
    parser.add_argument("--graph", type=Path, required=True)
    parser.add_argument("--profile", default="source-6000.3", choices=["source-6000.3", "compat-6000.3", "compat-2017.1"])
    parser.add_argument("--desktop", action="store_true", help="also replay the exact generated fixture in a real desktop window")
    args = parser.parse_args()
    output = work_path(ROOT, args.output or ROOT / ".work/snake-journey" / (time.strftime("%Y%m%d-%H%M%S") + "-" + uuid.uuid4().hex[:6]))
    output.mkdir(parents=True, exist_ok=False)
    summary = {"status": "inconclusive", "output": str(output), "profile": args.profile,
               "evidence_label": LABEL, "desktop_status": "not-run", "behavioral_checks": []}
    try:
        # The harness also enforces the 500GB worktree budget before canonical runs.
        usage = subprocess.run(["du", "-sb", str(ROOT)], capture_output=True, text=True)
        require(bool(usage.stdout.split()), f"cannot inspect worktree budget: {usage.stderr}")
        require(usage.returncode == 0 or all("No such file or directory" in line for line in usage.stderr.splitlines()),
                f"cannot inspect worktree budget: {usage.stderr}")
        summary["budget_scan_concurrent_file_removals"] = usage.returncode != 0
        summary["worktree_bytes"] = int(usage.stdout.split()[0])
        require(summary["worktree_bytes"] <= 500_000_000_000, "500GB hard worktree budget exceeded")
        runner = Worker(ROOT, output, args.worker.resolve(), metadata(ROOT), checkout("snake"), args.harness.resolve())
        if args.mode == "probe":
            timeline = [action(0, "load"), action(1, "advance", dt=0.125001),
                        action(1, "observe", selector=WORLD), action(1, "observe", selector="managed.snapshot"),
                        action(1, "capture", path="captures/probe.png"), action(1, "observe", selector=FOOD)]
            report = runner.run(timeline, args.profile)
            summary.update(status="inconclusive", message="provider probe only; complete journey not attempted",
                           food=observation(report, FOOD, 1), provider_report=str(runner.last_folder / "response.json"))
        else:
            execute_journey(runner, args, summary)
    except ProviderBlocked as error:
        summary.update(status="blocked", message=str(error))
    except (JourneyError, OSError, ValueError, KeyError) as error:
        summary.update(status="fail", message=str(error))
    write_json(output / "summary.json", summary)
    print(json.dumps(summary, indent=2), flush=True)
    return 0 if summary["status"] == "pass" else 2 if summary["status"] in ("blocked", "inconclusive") else 1


if __name__ == "__main__":
    raise SystemExit(main())
