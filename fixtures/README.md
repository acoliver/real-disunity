# Original-source fixtures

Executable JSON fixtures name an original game, source/API profiles, worker, scene, timeline and measured assertions. The configured compat-player worker compiles and executes the selected scripts through this repository's facade. Declaring a profile does not establish that it has executed or passed. Unattempted dashboard cells show no record. See the [harness protocol](../crates/compat-kit/README.md) and [player usage](../crates/compat-player/README.md).

| Fixture | Default profile | Original scene |
| --- | --- | --- |
| snake | source-6000.3 | Assets/Scenes/GameScene.unity |
| breakout | source-6000.3 | Assets/Scenes/SampleScene.unity |
| pong | source-2017.1 | Assets/Scenes/Main.unity |

Compatibility profiles target the selected Unity-facing API. They use this runtime's importer, facade and renderer. Fixture metadata declares source-specific mappings and visual substitutions, including authored missing-sprite replacements and font/material translations. Evidence labels and capture origins retain these distinctions. Original Unity repositories are never edited by this harness.

Smoke fixtures exercise load, input, advance, observations, capture, reload and unload. Journey fixtures add game-specific movement, collision, scoring or reset assertions. The keyboard planners and replay validators under `tools/compat-play`, `tools/compat-breakout` and `tools/compat-pong` check current observations and decoded captures. Harness-only protocol tests do not establish game compatibility.

## Source setup and checks

Set `"game"` to `"snake"`, `"breakout"` or `"pong"`. The kit resolves sources through [worldrunner-original-games](../crates/original-games/README.md), which fetches the pinned revision into `target/original-games` and verifies HEAD and a clean checkout before use. No fixture can select an arbitrary source directory. Shipped workers use `{root}/target/debug/compat-player`, independently of the kit's build target directory. Discovery accepts executable fixtures only. Unknown top-level fixture fields are rejected.

```sh
target/debug/compat-kit fixture check
target/debug/compat-kit fixture check snake
target/debug/compat-kit run snake --profile source-6000.3
```

Checks resolve the verified checkout, optional editor version, profile declarations and metadata. Worker requests contain the absolute checkout path. Runs retain measured observations, assertions, PNGs and teardown counters.

Pong fixtures use the repository-owned [DejaVu Sans font](fonts/DejaVuSans.ttf). Its [license text](fonts/DejaVu-LICENSE.txt) includes the Bitstream Vera copyright, trademark and redistribution conditions; DejaVu changes are public domain. This remains an explicit visual substitution in the fixture label.

Outputs stay in caller-selected, owned directories. Storage pruning removes only idle harness-owned runs and preserves active leases, foreign directories and trees with unsafe filesystem entries. Original-game storage is outside the prunable run tree. Missing source or executable prerequisites produce explicit blocked results.
