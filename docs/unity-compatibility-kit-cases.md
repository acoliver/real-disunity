# Compatibility runtime cases

This list groups the repository's executable tests and game fixtures by behavior. The linked sources define the assertions. It records available coverage without assigning a passing result to an unexecuted test. See the [architecture overview](unity-compatibility-kit-architecture.md) for ownership and the [fixture README](../fixtures/README.md) for run configuration.

## Native world and protocol

Native tests exercise the public ABI or JSON request API against the canonical world store.

| Case group | Behavior checked | Test sources |
| --- | --- | --- |
| Object and component identity | Distinct component handles, shared owners, stale generations, hierarchy, clone reference remapping and immediate native reads. | [gameplay](../crates/disunity-ffi/tests/gameplay.rs), [store](../crates/disunity-ffi/tests/store.rs) |
| Deferred destruction | Query visibility during a lifetime, pending ancestor destruction, component removal and handle retirement. | [deferred destruction](../crates/disunity-ffi/tests/deferred_destruction.rs) |
| Request integrity | Exact-byte retry behavior, retained errors, atomic transaction rollback, thread ownership and immutable reads. | [request integrity](../crates/disunity-ffi/tests/request_integrity.rs), [transaction rollback](../crates/disunity-ffi/tests/transaction_rollback.rs), [thread reads](../crates/disunity-ffi/tests/thread_reads.rs) |
| Clock and input | Fixed-step admission, pause, exact rational bounds, timestamp ordering, dynamic/fixed projections and focus transitions. | [clock](../crates/disunity-ffi/tests/clock.rs), [timeline](../crates/disunity-ffi/tests/timeline.rs), [input axes](../crates/disunity-ffi/tests/input_axes.rs) |
| Phase authority | Private host credentials, single-use claims, native stage order, cancellation, cleanup and stale-context rejection. | [phase contexts](../crates/disunity-ffi/tests/phase_contexts.rs), [session guards](../crates/disunity-ffi/tests/session_guards.rs) |
| Lifecycle and routines | Native eligibility, wait deadlines, cancellation, disposal, bounded nested starts and immediate lifecycle children. | [readiness](../crates/disunity-ffi/tests/readiness.rs), [nested start](../crates/disunity-ffi/tests/nested_start.rs) |
| Cleanup | Terminal takeover, conserved time/input, once-only cleanup attempts and response-admission rollback. | [terminal cleanup](../crates/disunity-ffi/tests/terminal_cleanup.rs), [readiness Cleanup](../crates/disunity-ffi/tests/readiness/cleanup.rs) |
| Immutable snapshots | Host-bound extraction, digest and identity validation, content revisions, retention bounds and release. | [content snapshots](../crates/disunity-ffi/tests/content_snapshots.rs), [snapshot admission](../crates/disunity-ffi/tests/snapshot_admission.rs) |
| Transport bounds | Unconditional per-world byte quotas, response ownership, admission rollback and independent response lifetimes. | [transport descriptor](../crates/disunity-ffi/tests/transport_descriptor.rs), [response budget](../crates/disunity-ffi/tests/response_budget.rs) |
| Physics and UI | Body participation, solid/trigger contacts, compound colliders, tile collision, rectangle layout and text fields. | [contact participation](../crates/disunity-ffi/tests/contact_participation.rs), [physics extensions](../crates/disunity-ffi/tests/physics_extensions.rs), [tile collision](../crates/disunity-ffi/tests/tile_collision.rs), [UI](../crates/disunity-ffi/tests/ui.rs) |

## Content import and materialization

Importer tests fetch pinned originals or create disposable source fixtures. They check imported values and emitted native operations. Python transport tests additionally load the native shared library.

| Case group | Behavior checked | Test sources |
| --- | --- | --- |
| Unity text and references | Multi-document YAML, GUID/file IDs, selected dependency traversal, malformed data and bounded reads. | [YAML](../crates/compat-content/tests/yaml.rs), [import](../crates/compat-content/tests/import.rs), [admission](../crates/compat-content/tests/admission.rs) |
| Prefabs and overlays | Nested expansion, scoped identities, overrides, removed components, field aliases and explicit missing-sprite substitutions. | [admission](../crates/compat-content/tests/admission.rs), [overlays](../crates/compat-content/tests/overlays.rs), [source profiles](../crates/compat-content/tests/source_profiles.rs) |
| Source reads and packaging | Owned source bytes, changed-input rejection, symlink rejection, script enumeration and canonical package inputs. | [closure controls](../crates/compat-content/tests/closure_controls.rs), [read drift](../crates/compat-content/tests/read_drift.rs), [source census](../crates/compat-content/tests/source_census.rs), [package](../crates/compat-content/tests/package.rs) |
| Native materialization | Complete references before activation, inactive templates, typed components, explicit omissions and error propagation. | [materialize](../crates/compat-content/tests/materialize.rs), [native transport](../crates/compat-content/tests/native_transport.py) |
| Tile geometry and UI | Authored cells, affine transforms, sprite bounds, invalid shape rejection, physical wall queries/bounces and typed UI fields. | [tile shapes](../crates/compat-content/tests/tile_shapes.rs), [native tile collision](../crates/compat-content/tests/native_tile_collision.py), [UI](../crates/compat-content/tests/ui.rs) |

## Managed execution and rendering

Runtime tests build native and managed artifacts themselves. Managed behavior checks run through the bridge and facade; synthetic probe assemblies exercise failure and boundary cases separately from original-game scripts.

| Case group | Behavior checked | Test sources |
| --- | --- | --- |
| ABI and negotiation | Generated binding layouts, native/bridge descriptors, unsupported requirements and load rejection. | [managed ABI](../crates/disunity-runtime/tests/managed.rs), [handshake](../crates/disunity-runtime/tests/handshake.rs), [managed declarations](../crates/disunity-runtime/tests/managed_facades.rs) |
| Script binding and lifecycle | Serialized references, private messages, activation changes, cloning, callback failures and root release. | [compatibility](../crates/disunity-runtime/tests/compatibility.rs), [callback failures](../crates/disunity-runtime/tests/callback_failures.rs), [managed behavior sources](../csharp/Compatibility.Tests) |
| Native-authorized dispatch | Phase contexts, lifecycle tickets, iterator operations, immediate children and selected Cleanup batching. | [phase contexts](../crates/disunity-runtime/tests/phase_contexts.rs), [readiness](../crates/disunity-runtime/tests/readiness.rs), [nested start](../crates/disunity-runtime/tests/nested_start.rs), [Cleanup consumer](../crates/disunity-runtime/tests/contextual_cleanup_consumer.rs) |
| Byte limits and host lifetime | Bounded managed transport, rejected oversized responses, retained native lifetime and host teardown. | [byte limits](../crates/disunity-runtime/tests/byte_limits.rs), [native lifetime](../crates/disunity-runtime/tests/native_lifetime.rs) |
| Player sessions | Original-source build, load/update/reload/unload, callback error reporting, strict timeline admission and interactive input. | [session](../crates/compat-player/tests/session.rs), [strict admission](../crates/compat-player/tests/strict_admission.rs), [interactive fixtures](../crates/compat-player/tests/interactive_fixtures.rs) |
| Camera and pixels | Shared projection and screen queries, sprite/image extraction, UI geometry, viewport changes and capture output. | [camera consumer](../crates/worldrunner/tests/resolved_camera.rs), [extraction tests](../crates/compat-player/src/extraction_tests.rs), [graphics tests](../crates/compat-player/src/graphics_tests.rs), [UI tests](../crates/compat-player/src/ui_tests.rs) |

## Game fixtures and journeys

The smoke fixtures cover load, input, native advancement, observations, capture, reload and unload. Journey fixtures add game-specific assertions. Interactive fixtures provide continuous desktop play. Source substitutions are explicit in [import observations](../crates/compat-content/SOURCE-OBSERVATIONS.md).

| Game | Fixtures and drivers | Journey behavior |
| --- | --- | --- |
| Snake | [smoke](../fixtures/snake.json), [journey](../fixtures/snake-journey.json), [interactive](../fixtures/snake-interactive.json), [planner and validator](../tools/compat-play/README.md) | WASD steering, reversal prevention, turns, horizontal/vertical wrap, food, deferred growth, movement cadence, self-collision, reload and renewed control. |
| Pong | [smoke](../fixtures/pong.json), [journey](../fixtures/pong-journey.json), [interactive](../fixtures/pong-interactive.json), [planner and validator](../tools/compat-pong/README.md) | Both paddles, physical bounces and goals, persistent score events, visible score changes, reset/wait/relaunch, reload and renewed control. |
| Breakout | [smoke](../fixtures/breakout.json), [controls](../fixtures/breakout-journey-controls-complete-compact.json), [multiball loss](../fixtures/breakout-journey-multiball-loss-compact.json), [width](../fixtures/breakout-journey-wide-complete-compact.json), [interactive](../fixtures/breakout-interactive.json), [planner and validator](../tools/compat-breakout/README.md) | Paddle clamps, original brick layout, physical paddle/brick/wall contacts, break animation/removal, ball loss and relaunch, wide-paddle expiry, reload and renewed control. |

Run a selected fixture with current builds. Desktop runs use the repository's display wrapper:

```sh
cargo build -j 2 -p worldrunner-disunity-ffi -p compat-player -p compat-kit
target/debug/compat-kit run snake-journey --profile compat-6000.3
python3 tools/compat-display/run.py -- target/debug/compat-kit run pong-journey --desktop
```

## Harness behavior and focused commands

Harness tests cover real subprocess supervision, cancellation, result statuses, assertion evaluation, confined capture paths, storage pruning, report exports and loopback HTTP security. Their generated images exercise report handling. Original-game behavior is checked by the game fixtures above. See [harness tests](../crates/compat-kit/tests) and [browser controls](../crates/compat-kit/tests/browser.mjs).

These focused commands run the corresponding suites with bounded Cargo parallelism. Managed suites require the .NET 8 SDK; renderer/player suites also require a GPU adapter and a display where applicable.

```sh
cargo test -j 2 -p worldrunner-original-games -p worldrunner-compat-content -- --test-threads=1
cargo test -j 2 -p worldrunner-disunity-ffi -- --test-threads=1
cargo test -j 2 -p worldrunner-disunity-runtime -- --test-threads=1
cargo test -j 2 -p compat-kit -- --test-threads=1
cargo test -j 2 -p compat-player -- --test-threads=1
python3 -m unittest discover -s crates/compat-content/tests -p 'native_*.py' -v
```

The standard CI commands are `cargo xtask ci-cpu`, `cargo xtask ci-managed` and `cargo xtask ci-gpu`. Build prerequisites and platform setup are in [build instructions](build.md). Results apply to the selected profiles, assertions and execution platform.
