# Lua scripts

`crates/script` hosts Lua 5.4. A host owns one Lua state, and scripts see only what the host installs:

- Lua's base library without `dofile` and `loadfile`, and the standard `string`, `table`, `math` and `utf8` libraries. There is no `os`, `io`, `debug` or `package`, so scripts have no filesystem and no wall clock.
- Source text only. `load` and every module compile text chunks and refuse precompiled bytecode, because Lua does not verify bytecode and a crafted chunk could corrupt the interpreter.
- The `worldrunner.*` modules the game installs, such as `worldrunner.world` and `worldrunner.assets`, and seeded random streams from `worldrunner.random.stream(name)`.
- `require`, which resolves only modules registered with the host.

Every hook fire runs under an instruction budget (`ScriptConfig::instruction_budget`). A script that exceeds it is halted with `ScriptError::BudgetExceeded`, so a runaway loop cannot hang a frame. The budget counts instructions, and a single `string.rep` can allocate gigabytes in one, so memory has its own limit: 256 MiB by default (`DEFAULT_MEMORY_LIMIT`), changed with `ScriptHost::set_memory_limit`. An allocation past it fails with a Lua memory error, which reaches the host as `ScriptError::Memory` unless the script catches it.

Support for running a game's original scripts is being added in [#147](https://github.com/acoliver/real-disunity/issues/147). Patch overlays are the first part.

## Patching a game's original scripts

A game keeps its original script files unchanged and ships patches beside them, for security fixes or updates to Lua 5.4. Patches apply as each file loads; the original files are never rewritten.

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
