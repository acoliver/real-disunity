# Compatibility player

`compat-player` runs an original Unity game's C# sources against the canonical native world. It compiles the game's scripts against the independently authored `UnityCompat` facade, hosts them in CoreCLR through `CompatibilityBridge`, drives the native frame loop, and renders native snapshots with the shared scene renderer. Engine services come through the `worldrunner` facade.

The player is the worker behind `compat-kit`: the kit selects a fixture, resolves the original game checkout, and sends the player one run request.

## Build and run

Requirements: the workspace Rust toolchain, the .NET 8 SDK, a Vulkan adapter, and an X11 display for desktop runs.

```sh
cargo build -p worldrunner-disunity-ffi -p compat-player -p compat-kit
target/debug/compat-kit fixture check
target/debug/compat-kit run snake
target/debug/compat-kit run pong-journey --profile compat-2017.1
python3 tools/compat-display/run.py -- target/debug/compat-kit run snake-interactive --desktop
target/debug/compat-kit serve --port 8787
```

The player loads `libworldrunner_disunity_ffi` from the directory it was built into, so build both crates with the same profile. `DOTNET_HOST_PATH` selects a specific `dotnet`; otherwise `dotnet` is taken from `PATH`.

Original games are fetched at pinned revisions by `worldrunner-original-games` and never modified. Each fixture names its game, scene, script count, import profile and any explicit content substitutions. Snake uses an authored replacement for its missing sprite. Pong uses legacy field and UI mappings and the bundled DejaVu Sans font. Breakout declares its unlit material translation and sprite-bounds wall collision.

Compatibility profiles (`compat-6000.3`, `compat-2017.1`) select the facade's API surface on .NET 8. They do not claim equivalence with a historical Unity editor or compiler.

## Desktop play

The `*-interactive` fixtures play continuously in a window:

| Game | Keys |
| --- | --- |
| Snake | W/A/S/D |
| Pong | W/S and the arrow keys |
| Breakout | A/D |

F5 reloads, F12 captures, and Escape or closing the window stops. Reload clears held keys and axes in both the driver and the native session. Focus loss releases held keys.

Interactive metadata is a strict object:

```json
"interactive": {"duration_ms": 30000, "max_frames": 3600, "max_captures": 8}
```

It requires desktop execution and a timeline containing exactly one `load` at frame zero. Each live tick advances the session by the actual frame delta, subdivided into steps of at most 1/60 second, with at most 250 ms and 15 steps per tick. `interactive-events.ndjson` in the run directory logs input, deltas, admitted steps, reloads and presented frames.

## Worker protocol

The harness sets `COMPAT_ROOT`, `COMPAT_REQUEST` and `COMPAT_RESPONSE`. A request carries the fixture, profile, pinned source root, timeline, assertions and metadata. Timeline actions are `load`, `input`, `advance`, `observe`, `capture`, `reload` and `unload`.

Native `frame.begin` admits fixed work and advances the clock. The player calls script phases around native physics, delivers native contacts, and renders immutable snapshots. Destroyed objects stay visible to native queries until the end-of-frame flush.

The first session in a process copies the native library into `.work/compat/native-images` and every later session reuses that copy, so a concurrent `cargo build` cannot replace the loaded image.

Observations are `world.snapshot`, `managed.snapshot`, `events`, `object:<name>` and `managed:<type>:<member-path>`. Unsupported observations fail. Teardown reports the remaining native worlds and managed roots, which must both be zero.

Fixture metadata `managed_configuration` selects `Debug` (default) or `Release` for the bridge, facade and game. `profile.extraction` and `profile.presentation` enable per-frame timing logs in the run directory.

## Tests

```sh
cargo test -p compat-player --all-features -- --include-ignored --test-threads=1
```

Tests build the native library and managed suites themselves and fetch the pinned original games. Session tests load Snake, update it, reload and tear down with zero native worlds and managed roots. Desktop tests drive live ticks and OS input through the same session.
