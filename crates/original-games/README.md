# Pinned original Unity games

`worldrunner-original-games` owns the source URLs and exact revisions used by engine tests and tools. Consumers select `Game::Snake`, `Game::Breakout` or `Game::Pong` and call `checkout(game)` for a read-only source path.

```rust
use worldrunner_original_games::{checkout, Game};

let root = checkout(Game::Pong)?;
assert!(root.join("Assets/Scenes/Main.unity").is_file());
# Ok::<(), worldrunner_original_games::Error>(())
```

`Game::ALL` lists the three selections. `name()`, `rev()` and `url()` expose their pinned identities. Parsing a game name through `FromStr` accepts only those names.

`checkout` finds the workspace through `CARGO_MANIFEST_DIR` ancestry. `checkout_in(workspace, game)` selects an explicit workspace. Both use `<workspace>/target/original-games/<name>-<rev>` independently of the build target directory. `workspace_root()` exposes the discovered root.

On a cache miss, the fetcher holds a per-game file lock while system Git initializes a temporary directory, adds the pinned remote, fetches the exact revision with depth one and checks out `FETCH_HEAD`. It verifies HEAD and a clean working tree before publishing the directory with a rename. Failed staging directories are removed. Repeated calls verify existing checkouts without fetching or updating them. A changed revision or working tree is an error; the fetcher leaves that directory untouched.

## Command line

Run these commands from the workspace:

```sh
cargo run -p worldrunner-original-games --bin original-games -- path pong
cargo run -p worldrunner-original-games --bin original-games -- fetch-all
cargo run -p worldrunner-original-games --bin original-games -- --workspace /path/to/workspace path snake
```

`path` prints one path. `fetch-all` prints one path for each game in `Game::ALL` order. Git diagnostics go to stderr on failure. The cache requires system Git and network access on the first fetch.

## Tests

Unit tests check pin metadata, workspace paths, Git error context and rejection of modified or wrong-revision repositories. One integration test fetches all three originals into a disposable workspace, exercises concurrent callers and both CLI commands, verifies exact revisions, checks for `Assets/Scenes/*.unity`, and checks that cached reads leave Git HEAD untouched.

```sh
cargo +1.97.0 test -p worldrunner-original-games -- --test-threads=1
```
