# Native request integrity

The existing ABI 2 exports operate on one process-wide native world store. `describe` remains pure and available with world zero or an exact live world handle. The JSON protocol exposes request retention, atomic transactions and immutable queries over that store. See the [runtime architecture](../../docs/unity-compatibility-kit-architecture.md) for managed and player integration.

## Retained request identities

A request can opt into session-scoped retention with an explicit version and string identity:

```json
{"version":1,"requestId":"create-1","op":"object.create","name":"example"}
```

For each admitted identity, native storage retains the exact incoming request bytes and encoded semantic response until that world is destroyed. An identical-byte retry returns an independently owned response containing the original bytes. Response probing, copying or freeing never releases the retained identity. Retrying creation after its original object was destroyed returns the original retired handles and performs no new creation. Retrying an event drain returns its original contacts without draining later contacts.

Reusing an identity with different bytes returns an `invalid_request` semantic response without changing the world or replacing the ledger entry. Whitespace, field ordering and equivalent JSON number spellings count as different bytes. The Rust `request_value` entry point uses its JSON serializer's output as its request bytes and shares the same session ledger. Cross-transport callers must preserve that encoding when retrying a shared identity.

A valid version-1 identity retains semantic validation errors, operation errors, budget errors represented as semantic responses, and successful read results. An invalid envelope, malformed JSON, wrong-thread admission, invalid world, oversized input, exhausted response storage or exhausted ledger does not admit an identity. Retrying these preadmission failures may succeed after the admission condition changes. A semantic failure returned with an admitted identity is stable after later world changes.

The [ledger](src/ledger.rs) has these per-world limits:

| Limit | Value |
| --- | --- |
| Retained identities | 1,024 |
| Identity length | 1 through 128 UTF-8 bytes |
| Retained bytes | 32 MiB |
| New-identity response reservation | 16 MiB |

Byte accounting includes the identity string, exact request bytes, and encoded response bytes. Before executing a new identity, admission requires room for its identity, request and a worst-case 16 MiB response. After execution, actual encoded response size replaces that reservation. This conservative rule can reject new identities while fewer than 32 MiB are occupied. Collection metadata is additionally bounded by the identity-count limit. There is no eviction and no release-identity operation. Existing exact-byte replays bypass new-identity capacity checks. A full ledger returns transport `BUDGET` with response handle zero before world mutation; requests without identities remain available. World destruction releases the ledger. A new world generation has an independent ledger.

## Atomic operation batches

Atomic batches are separate from legacy queued commands and require explicit version 1:

```json
{"version":1,"requestId":"batch-1","op":"transaction","operations":[{"op":"object.create","name":"example"},{"op":"frame.begin","dt":0.02}]}
```

A transaction contains 1 through 128 operations. All operation shapes are parsed before execution. Nested transactions and suboperation envelopes, identities or phase credentials are rejected. Operations execute sequentially against one staged clone of the canonical world, so later operations see earlier staged changes. Existing object and component handles can be referenced normally. There is no temporary-handle or result-reference substitution syntax. Success returns an ordered array of operation data and commits the staged world once. An outer identity retains the entire success or failure response.

Operation failure, fixed-step admission budget failure, or an oversized aggregate response discards staging. This preserves native objects/components and their live generations, hierarchy, input and axis configuration, clock and accumulator, random state, physics contacts/events/layer masks, deferred destruction, UI resources, queued legacy commands and legacy diagnostics. Process-wide allocation generations remain monotonic even when staging fails; discarded allocations never revive earlier handles.

Aggregate result bytes are counted as each operation completes, including array separators and the response envelope. Crossing the 16 MiB response limit terminates result accumulation and returns `response budget exceeded; operation rolled back`. The later operations are not executed. The final serialized envelope is checked again before commit. Staging clones world state only. Session ownership and the retained ledger live outside the clonable world, preventing recursive ledger copies. Typed immutable reads and parse errors avoid cloning the world. A single large query result is still materialized before its size is checked, and mutation staging remains proportional to world state; this slice does not establish a total world-memory quota.

The old `disunity_command_buffer_submit` / `disunity_world_step` adapter keeps its queued, partially applied batch behavior. On a bad command, a valid prefix remains applied, the failed batch tail is discarded, the error remains fetchable, and the clock/solver are not advanced past that command failure. The new transaction operation does not change those legacy semantics.

## Thread ownership and immutable reads

The thread calling `disunity_world_create` owns the world until destruction. ABI object creation/destruction, command submission, transform setters, world stepping/destruction, and protocol mutations require that exact thread. Cross-thread legacy mutation returns `INVALID_ARGUMENT`; protocol mutation returns an `invalid_request` response with `world mutation requires its owner thread`. Rejection changes neither the world, command queue, last legacy diagnostic nor ledger.

Retained reads and retained replays also require the owner, since retention belongs to the session's mutation contract. A read-only transaction requires the owner. Same-thread writes and immediate reads remain supported. Native code invokes no managed callbacks while holding its store locks.

Cross-thread protocol access permits only unretained typed immutable queries: `describe`, `session.status`, `snapshot`, `lifecycle.snapshot`, `lifecycle.scripts`, `object.alive/get/find`, `component.alive/get/list`, `transform.get`, `input.get/status`, `time.get`, `physics.overlap/raycast`, `ui.rect/layout`. Handle and response budgets still apply. `events.read` requires the owner even with `clear:false`; use the owner for all contact access. ABI bulk export, error reads and store statistics remain readable across threads. Response copy/free are independent of owner thread and continue to work after world destruction. These calls are serialized by existing native locks; no lock-free or wait-time guarantee is advertised.

## Fresh script eligibility

Providers advertising `lifecycle_scripts` accept `{"version":1,"op":"lifecycle.scripts"}`. The data is an array of `[component_handle, active_in_hierarchy, enabled]` tuples for every live Script component, in native component storage order. Each request reads the canonical world without staging a clone. It contains no component fields or non-script identities. Pending destruction remains visible until the existing lifetime scope ends. Hierarchy changes, enable changes, clone generations and committed transactions are reflected immediately. Rollbacks leave the result unchanged. Request identities intentionally retain the original result under the existing ledger contract, so lifecycle consumers use unretained requests for fresh authority.

The original `lifecycle.snapshot` contract remains unchanged, including complete component handle/object identities used to validate contact liveness. The bridge selects the compact projection from the immutable provider descriptor once per session and falls back to the original query on older providers. Snapshot data stays local to a phase or reconciliation pass. Every actual script callback and coroutine cleanup can mutate peers and requires a fresh read before eligibility is reused. This query adds no revision cache or native scheduler.

## Advertised capabilities and execution profiles

The descriptor adds `request_deduplication`, `atomic_transactions`, `thread_affinity`, and `lifecycle_scripts` to the existing capabilities and reports their actual limits and policies. The [native timeline/input S1 contract](TIMELINE.md) adds opt-in `rational_time`, `unscaled_time`, and `timestamped_input`. The default clock policy remains `float_accumulated` for existing consumers; `rational_clock_policy:explicit_bounded_exact` describes explicit rational selection.

The [transport byte limits](TRANSPORT.md) apply to every world: 1 MiB requests, 8 MiB replies, 16 MiB outstanding reply bytes per world generation and 1,024 process-wide response handles. Retained semantic results have a separate 16 MiB per-world quota and reserve an 8 MiB result plus request and ID bytes before execution. Exact reply admission completes before staged mutations commit.

The [phase-context contract](PHASE_CONTEXT.md) exposes `native_phase_context_native_v1`: native-issued session/phase credentials, actual-world identity, single-use claims, host-driven frame order, advancement guards and pre-script fixed input projection. The selected profile is `native_phase_context_v1`. These controls reject requestId and transaction nesting. [Readiness](READINESS.md), [terminal cleanup](TERMINAL_CLEANUP.md) and [immutable snapshots](SNAPSHOTS.md) define additional contracts. The [managed runtime](../disunity-runtime/src/compatibility.rs) negotiates selected profiles with the Cdecl bridge. `phase_tokens` and `native_scheduler_v1` are absent from the native descriptor.
