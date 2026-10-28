# Lua scripts

`crates/script` hosts Lua 5.4. A host owns one Lua state, and scripts see only what the host installs:

- Lua's base library without `dofile` and `loadfile`, and the standard `string`, `table`, `math` and `utf8` libraries. There is no `os`, `io`, `debug` or `package`, so scripts have no filesystem and no wall clock.
- Source text only. `load` and every module compile text chunks and refuse precompiled bytecode, because Lua does not verify bytecode and a crafted chunk could corrupt the interpreter.
- The `worldrunner.*` modules the game installs, such as `worldrunner.world` and `worldrunner.assets`, and seeded random streams from `worldrunner.random.stream(name)`.
- `require`, which resolves only modules registered with the host.

Every hook fire runs under an instruction budget (`ScriptConfig::instruction_budget`). A script that exceeds it is halted with `ScriptError::BudgetExceeded`, so a runaway loop cannot hang a frame. The budget counts instructions, and a single `string.rep` can allocate gigabytes in one, so memory has its own limit: 256 MiB by default (`DEFAULT_MEMORY_LIMIT`), changed with `ScriptHost::set_memory_limit`. An allocation past it fails with a Lua memory error, which reaches the host as `ScriptError::Memory` unless the script catches it.

Support for running a game's original scripts is being added in [#147](https://github.com/acoliver/real-disunity/issues/147). The profile, threads and timers, saving Lua data and threads, and patch overlays below are done. Saving closures, and threads that resume mid-function, need a decision recorded on that issue.

## Running a game's original scripts

`ScriptHost` is the strict profile for scripts written for this engine. A game's own scripts expect the API their original engine exposed, so they run in `GameScripts`, which has the same sandbox, budget and memory limit and adds the `coroutine` library:

```rust
use worldrunner_script::{GameScripts, ScriptConfig};

let mut scripts = GameScripts::new(ScriptConfig::default())?;
scripts.register_native("GetMapName", |_, ()| Ok("Crater"))?;
scripts.register_native("terrain.GetHeight", |_, (x, y): (f64, f64)| Ok(height(x, y)))?;
scripts.register_native("SaveFile", |_, path: String| save(&path))?;
scripts.add_source("game", &["GetMapName", "terrain.GetHeight", "SaveFile"])?;
scripts.add_source("mod:better-ui", &["GetMapName"])?;
scripts.run("game", "Lua/Map.lua", &map_lua)?;
scripts.run("mod:better-ui", "UI.lua", &mod_lua)?;
let title: String = scripts.call("MapTitle", ())?;
```

- **Natives** are the game's Rust functions, registered under a name or a dotted path such as `terrain.GetHeight`. They are all registered before the first source, and a path is either a function or a namespace, never both.
- **Sources** are the groups of scripts the game distinguishes: its own files, its patches, each mod. A source can call exactly the natives the game grants it.
- **Globals are shared.** Every global a script reads or writes that is not one of its natives lives in one namespace shared by all sources, as in the original engine: game files define globals that other files and mods use. A native namespace such as `terrain` shows the game's own `terrain` entries next to the natives, and the natives stay visible however the game rebinds or extends `terrain`.
- **Capabilities bound direct access.** A function keeps the natives of the source that defined it, so a mod calling a game function runs game code with the game's natives, and a source can hand a native it holds to others. Code compiled with `load` runs in the shared namespace, which holds no natives. Sources can replace each other's functions, as they could in the original engine.
- **Coroutines run under the budget.** Instructions inside coroutines, however deeply nested, count against the budget of the `run` or `call` that resumed them. `coroutine.resume` returns an error's message without a stack traceback, as Lua's does; an error value that is not a string arrives as its text.
- `run` compiles source text only, and `run_patched` applies the file's patches first. `call` invokes a function in the shared namespace, as the engine calls a game's entry points; each `run` and `call` gets the full instruction budget.

## Threads and timers

Original engines run game logic in coroutines that sleep on the game clock. `GameScripts` has one scheduler, and the game binds its functions under the names its original engine used, as natives granted like any other:

```rust
use worldrunner_script::SchedulerNames;

scripts.register_scheduler(&SchedulerNames {
    spawn: "CreateGameTimeThread".into(),
    sleep: "Sleep".into(),
    wakeup: "Wakeup".into(),
    kill: "DeleteThread".into(),
    current: "CurrentThread".into(),
    now: "GameTime".into(),
})?;
// After sources are added and scripts run, once per game tick:
let failures = scripts.advance(tick_ms)?;
```

| Function | Behaviour |
| --- | --- |
| `spawn(f, ...)` | Creates a thread that calls `f(...)`, first running at the next `advance`. Returns the thread. |
| `sleep(ms)` | Suspends the calling thread for `ms` milliseconds, rounded up. `math.huge` sleeps until woken. Negative or non-numeric times are errors. |
| `wakeup(thread)` | Makes a sleeping thread due now. Returns whether it was waiting. |
| `kill(thread)` | Removes a thread, including the running one. Returns whether it existed. |
| `current()` | The running scheduler thread, or nil outside one. |
| `now()` | The scheduler clock in milliseconds. |

`advance(ms)` moves the clock and runs every thread due by then, in order of wake time and then of scheduling. A thread sees the clock at its wake time, so one that sleeps again within the advanced span runs again within it. `sleep(0)` and any other plain `coroutine.yield` wait for the next `advance`, so a loop of zero sleeps cannot spin within one. Each resume has the full instruction budget. An error ends only the thread that raised it and is returned in `advance`'s failure list; the other threads keep running. `sleep` outside a scheduler thread is an error.
## Saving Lua state

A game saves its Lua data with `save` and restores it, into a fresh `GameScripts` that has run the same scripts, with `restore`:

```rust
// Save the tables at these global paths. Everything under `Classes` is code.
let snapshot = scripts.save(&["GameState", "Score"], &["Classes"])?;
let bytes = serde_json::to_vec(&snapshot)?;

// Later, in a new state that has registered the same natives and run the game's scripts:
let snapshot: LuaSnapshot = serde_json::from_slice(&bytes)?;
scripts.restore(&snapshot, &["Classes"])?;
```

- **Data is copied, with its shape.** Tables shared by reference stay shared, cycles survive, metatables are kept, integers stay integers and floats stay floats, including NaN and infinities, and strings keep their exact bytes.
- **Code is saved by name.** Natives, and every function and table reachable through string keys from the permanent roots the game names, are permanents. A snapshot stores only their names, such as `Classes.Unit.Hit`, and `restore` looks each name up in the current state. A restored save therefore runs the game's current code. A name that no longer exists fails the restore and is reported.
- **Scheduler threads are saved as data:** the permanent function each was spawned with, its arguments and its wake time, together with the scheduler clock. Every waiting thread is saved, whether or not the roots refer to it, and a thread value stored in the data restores as the same restored thread. A restored thread starts its function again from the beginning at its saved wake time, so state a thread must keep across a save belongs in tables, where Haemimont-style game code keeps a unit's state anyway. A thread spawned with a closure cannot be saved. Finished coroutines restore as finished.
- **Restoring threads needs an empty scheduler.** Restore saved threads before the game spawns any. A snapshot without threads restores into a running scheduler and leaves its threads and clock alone.
- **Not yet supported:** functions that are not permanents, such as closures created at run time, and suspended coroutines the scheduler does not own. Saving one fails and names where it was found, for example `GameState["units"][3]["on_death"]`. Resuming a thread mid-function after a load needs a serializer for Lua's internals; see [#147](https://github.com/acoliver/real-disunity/issues/147).
- `LuaSnapshot` implements serde, so the game chooses the encoding. Snapshots of the same state are identical: string, number and boolean keys are written in sorted order. Only tables used as keys keep Lua's iteration order.
- `restore` assigns each root to its global path; the parent of a dotted path must already exist.

## Patching a game's original scripts

The existing Lua API supports load-time patches, for example for security fixes or Lua 5.4 updates, without rewriting source files. This describes tool functionality. The [contribution policy](../CONTRIBUTING.md#start-with-a-fork-and-a-scoped-change) requires unchanged original scripts, assets and behavior apart from API binding and harness configuration; it does not authorize per-game overlays for compatibility contributions. Source inspection and redistribution still require the particular game's permissions.

```rust
use worldrunner_script::{Edit, Overlay, Patch, PatchSet, ScriptConfig, ScriptHost};

let overlay = Overlay::new([PatchSet {
    file: "units/sum.lua".into(),
    // `sha256sum units/sum.lua` of the original file.
    original_sha256: "9f86d0…".into(),
    patches: vec![Patch {
        name: "lua54-varargs".into(),
        edits: vec![Edit {
            find: "  local n = 0\n".into(),
            replace: "  local arg = {...}\n  local n = 0\n".into(),
        }],
    }],
}])?;
let mut host = ScriptHost::new(ScriptConfig::default())?;
let applied = host.load_patched_module("units/sum.lua", &original_bytes, &overlay)?;
```

The rules:

- **Each file's patches are pinned to the SHA-256 of its original bytes**, written as `sha256sum` prints it. If the original changes, for example after a game update, loading fails with `PatchError::Stale`, which names the file and both hashes. The patches then need review against the new file; they are not applied to it, and the file does not run unpatched in their place.
- **An edit replaces one exact text with another, and must match exactly once** in the text as it stands when the edit applies. Zero or several matches fail with `PatchError::Match`, naming the patch and edit. Matching is byte for byte, so line endings must match the original.
- **Order is significant.** Edits apply in order within a patch, and patches apply in order, so a later edit sees the text earlier ones produced.
- **A file without a patch set loads unchanged.**

`PatchSet` implements serde, so a game can keep its patches as JSON or TOML in its own repository. `Overlay::review` checks every patch set against the game's current files at once and returns every problem, which suits a check after a game update.
