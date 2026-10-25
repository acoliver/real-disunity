# Native immutable snapshot primitives v1

`snapshot_primitives_v1` provides immutable extraction on the canonical native world store, ABI 2, protocol 1 and the existing 15 exports. It uses [phase-context authority](PHASE_CONTEXT.md). Schedule `revision` counts phase transitions; `content_revision` is a separate checked u64 that tracks accepted content mutations. [Snapshot storage](src/snapshots.rs) retains stamp metadata while the caller owns the returned bytes.

## Wire contract

All three operations reject unknown fields, requestId and transaction nesting. They require the world's owner thread and the exact private `host:{session,host_key}` returned by session.open. Public stamps and status omit that key and phase tokens. A closed session cannot authenticate snapshot operations.

```text
snapshot.capture {host} -> {stamp, snapshot}
snapshot.validate {host, stamp, require_current:bool} -> {valid:true, current:bool}
snapshot.release {host, stamp} -> {released:true}
```

The common `op` field names the operation. Protocol version may be explicitly 1. Semantic rejections use `{version:1,ok:false,error:{code:"invalid_request",message}}`. Transport admission returns the existing ABI budget/invalid-handle codes and response handle zero on failure.

Capture requires initialized Idle, no active phase root and drained readiness execution. It rejects pre-Initialize, claimed/unclaimed roots, Commit, End, Cleanup and Closed. Idle immediately after Initialize and Idle after a completed frame are eligible. The host separately decides whether application loading and rendering are ready.

The stamp has exactly these fields, with strict unsigned integer deserialization:

```text
world: u64
world_generation: u32
session: string
frame: u64
fixed_tick: u64
schedule_revision: u64
content_revision: u64
snapshot_id: u64
snapshot_sha256: string
```

World and world_generation come from the native generational world handle. Session comes from native authority. Frame and fixed_tick are committed native counters. Snapshot IDs increase with checked arithmetic per world and are never reused after successful issuance, including after release. Failed unexposed staging may reuse its tentative ID. World generation and session identity prevent cross-world aliases.

Validation authenticates the host, checks world/session identity, and requires equality with every retained stamp field, including digest and ID. `require_current:true` additionally requires current content_revision, schedule_revision, frame and fixed_tick. `false` permits a retained snapshot after native advancement and returns `current:false`. Validation grants no phase authority. Release consumes that retained identity once; repeated release and later validation reject. Closing the session revokes authentication, and world destruction drops retained metadata. Owned response bytes remain readable until response_free, including after world destruction.

## Snapshot bytes and digest

Capture uses the canonical snapshot serializer for objects, component metadata, transforms, time, dynamic input and public session status. RNG state, queues and some infrastructure state remain internal, while their accepted mutations still advance content_revision. Public retention counts are diagnostic metadata excluded from content revision. Capture and release can change those counts without invalidating a content stamp.

`snapshot_sha256` is lowercase SHA-256 of the compact UTF-8 JSON bytes of the `snapshot` value inside the returned response. Object keys follow serde_json sorted-map serialization. The envelope and stamp are excluded. Hashing a complete response, pretty-printing or reserializing numbers can produce a different digest. Extract the raw snapshot value with a JSON parser that retains source spans, then hash those exact bytes. The private [digest implementation](src/snapshots/digest.rs) includes standard SHA-256 test vectors.

Native validates the retained digest. A stamp-only request cannot detect edits to a separately copied snapshot. The host must hash the bytes it uses. Native snapshot validation establishes neither GPU pixel identity nor presentation completion.

## Bounds and atomicity

Each world retains at most 128 stamps and 32 MiB of represented snapshot bytes. Storage holds stamp and length metadata, without an additional snapshot payload or world cache. There is no automatic eviction. Release recovers both quotas. Snapshot IDs use one checked counter without an unbounded tombstone history.

Capture and release use session staging: clone the canonical world, execute, serialize the bounded response, then commit. An operation error, oversized serialized response or transport-admission failure commits no stamp, ID, content revision or gameplay change. Transport reserves worst-case capacity before execution. The Rust request_value path also serializes and checks response size before committing.

The [transport byte limits](TRANSPORT.md) are 1 MiB per request, 8 MiB per response, 16 MiB outstanding response bytes per world generation and 1024 process-wide response handles. Copy/free has no authority or revision side effect and is cross-thread safe. Releasing snapshot metadata and freeing its response are independent operations; hosts must manage both lifetimes.

## Content revision commit boundary

Typed request classification stages one checked increment before a content-mutating top-level operation. On success, response serialization precedes commit. Internal helpers and transaction suboperations share that outer increment. Failed requests, rollback, serialization failures and response-admission failures do not advance it. Exact-byte ledger replay returns before staging. Successful mutating no-ops conservatively advance it.

| Request family | Content revision behavior |
| --- | --- |
| Immutable queries, including describe, status, lifecycle, object/component/TRS, input/time, snapshot, overlap/raycast and UI | None |
| Object create/set/destroy/clone, TRS/parent changes, component add/set/remove | One per successful outer request |
| Lifetime and delayed destruction controls | One |
| Input held/axes/focus/configuration, random seed/range, clock/phase controls | One |
| Rational frame admission and timestamped input | One |
| Body, layer, gravity, solver and clearing events.read | One |
| events.read clear:false | None; owner-affinity and retention rules still apply |
| Font registration and UI viewport configuration | One |
| Session, frame, phase and fixed-commit controls | One, including control-only transitions |
| Readiness and callback-batch transitions | One, including successful no-op/duplicate acknowledgment |
| Transaction | One if any suboperation mutates; zero for read-only transactions or rollback |
| Snapshot capture/validate/release | None; retained extraction lifetime is independent |

The ABI object and command writes have checked revision barriers after authority and argument validation. Transform setters delegate to command-buffer submission and share its increment. ABI version, error reads, statistics, exports and response copy/free do not mutate content.

Legacy world_step preserves partial-commit behavior: an admitted batch can retain a valid prefix, discard a failed tail and return an error. It advances content revision once after admission, including on that partial error. Invalid time, wrong thread, rational-mode rejection and strict authority rejection do not advance it. Strict sessions reject legacy world_step. Protocol mutations and strict fixed.commit use atomic staging.

## Runtime consumption

[CompatibilityHost snapshot methods](../disunity-runtime/src/compatibility/snapshot.rs) require an owned strict world and negotiate `snapshot_primitives_v1`. `capture_snapshot` copies the native response, verifies SHA-256 over its raw snapshot slice and returns `CapturedSnapshot`. Its accessors expose the exact raw bytes, immutable numeric projection and `SnapshotStamp`. `validate_snapshot` checks retained/current identity, and `release_snapshot` releases the native metadata.

Keep host credentials private and extract snapshot plus stamp from the same response. Validate current identity before work that requires current native state. For asynchronous work after advancement, validate with `require_current:false` and keep the original extraction until completion. Release its native metadata and response storage independently. Camera, viewport, color layout and GPU completion belong to the rendering consumer; they are absent from the native stamp.
