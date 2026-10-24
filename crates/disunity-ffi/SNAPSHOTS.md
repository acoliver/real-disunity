# Native immutable snapshot primitives v1

`snapshot_primitives_v1` qualifies a native-only immutable extraction and receipt API on the existing canonical STORE, ABI 2, protocol 1 and 15 exports. It uses the existing `native_phase_context_v1` authority. It adds no scheduler, world backend, render state, managed integration, provider activation or pixel attestation. Existing `revision` remains the phase schedule-transition counter. `content_revision` is a separate checked u64.

## Wire contract

All three operations reject unknown fields, requestId and transaction nesting. They require the world's owner thread and the exact private `host:{session,host_key}` returned by session.open. Public stamps and status never contain that key or phase tokens. A closed session cannot authenticate snapshot operations.

```text
snapshot.capture {host} -> {stamp, snapshot}
snapshot.validate {host, stamp, require_current:bool} -> {valid:true, current:bool}
snapshot.release {host, stamp} -> {released:true}
```

The common `op` field names the operation. Protocol version may be explicitly 1. Ordinary semantic rejections use the existing `{version:1,ok:false,error:{code:"invalid_request",message}}` envelope. Transport admission continues returning the existing ABI budget/invalid-handle codes and response handle zero on failure.

Capture requires initialized `Idle`, no active phase root and drained readiness execution. It rejects pre-Initialize, claimed/unclaimed roots, Commit, End, Cleanup and Closed. Idle immediately after Initialize is eligible, as is Idle after a completed frame. This is an extraction boundary. It makes no claim that scripts, streaming or temporal rendering have reached application readiness.

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

World and world_generation come from the real native generational world handle. Session comes from native authority. Frame and fixed_tick are committed native counters. Snapshot IDs increase with checked arithmetic per world and are never reused after successful issuance, including after release. Failed unexposed staging may reuse its tentative ID. Actual world generation and session epoch prevent cross-world aliases.

Validation authenticates the host, checks actual world/session identity, and requires exact equality with retained native receipt metadata for every stamp field, including digest and ID. `require_current:true` additionally requires current content_revision, schedule_revision, frame and fixed_tick. `false` allows an old retained identity after native advancement and reports current:false. It establishes historical identity for asynchronous completion. It grants no phase authority. Release consumes that exact receipt once. Repeat release and all later validation reject. Closing the session revokes authentication; world destruction drops all receipts. Existing owned response bytes remain readable until response_free, including after world destruction.

## Snapshot bytes and digest

Capture calls the existing canonical snapshot serializer. It returns objects, component metadata, transforms, time, dynamic input and the existing public session status. It does not serialize the complete internal world: RNG state, queues and some infrastructure state remain internal. Their accepted mutations still advance content_revision. Public receipt counts in session status are diagnostic metadata, excluded from content revision. Consequently capture/release may change diagnostic counts without invalidating a content stamp.

`snapshot_sha256` is lowercase SHA-256 of the exact compact UTF-8 JSON bytes of the `snapshot` value as serialized inside the returned response. Object keys follow this crate's existing serde_json sorted-map serialization. The envelope and stamp are excluded. Hashing the complete response, pretty-printing, or reserializing numbers with a different JSON implementation can produce a different digest. Extract the raw snapshot value slice from the copied response using a JSON parser that retains source spans, and hash that slice. `.work/resumed-native-snapshots/probe.py` contains an executable exact-byte extraction example, checked against Python hashlib with Unicode and exponent-valued transforms.

The frozen workspace declares sha2 globally but omits it from this crate's locked dependency set. Changing Cargo.lock was outside this slice's ownership. A small private FIPS 180-4 SHA-256 implementation avoids dependency or lock changes. Standard empty, abc, multiblock and million-a vectors and independent loaded hashlib comparison cover it. The digest is an identity checksum; host authentication and exact native receipt equality enforce authorization. This implementation is not a new signature or MAC scheme.

Native validates the original receipt digest. A stamp-only request cannot detect host edits to a separately copied snapshot. The trusted host must hash the actual extracted bytes before using them. Native cannot verify GPU pixels. No GPU or presentation completion is inferred from receipt validation.

## Bounds and atomicity

Each canonical world retains at most 128 receipts and at most 32 MiB of represented snapshot bytes. Receipt storage holds stamp and length metadata only. It stores no additional snapshot payload or world cache. There is no automatic eviction. Release recovers both quotas. Snapshot IDs use one checked counter, with no unbounded tombstone history.

Capture and release use existing Session staging: clone canonical world, execute, serialize the complete bounded response, then commit. A failed operation, oversized serialized response or transport response-admission failure commits no receipt, ID, content revision or gameplay change. Transport reserves worst-case capacity before execution. The Rust request_value path also serializes and checks response size before committing.

The existing independent response-store limits remain 1 MiB per request, 16 MiB per response, 64 MiB live response bytes and 1024 response handles. Copy/free has no authority or revision side effect and is cross-thread safe. Receipt release does not free a response; response_free does not release a receipt. Hosts must manage both lifetimes.

## Content revision commit boundary

The protocol uses exhaustive typed Request classification and the existing single execution implementation. Session stages a checked increment before executing each content-mutating top-level request. On success it serializes the response before committing the staged state. Internal helper mutations and transaction suboperations share that one outer increment. Failed requests, failed transactions, serialization failures and response-admission failures do not advance. Exact-byte ledger replay returns before staging and does not advance. Successful mutating no-ops conservatively advance.

| Request family | Content revision behavior |
| --- | --- |
| ReadRequest, including describe, session.status, lifecycle, object/component/TRS queries, input/time, snapshot, overlap/raycast and UI queries | None |
| Object create/set/destroy/clone | One per successful outer request |
| TRS and parent changes | One |
| Component add/set/remove | One |
| Lifetime begin/end and delayed destruction controls | One |
| Input held/axes/focus/configuration, random seed/range, legacy clock/phase controls | One |
| Rational configuration/frame begin and timestamped input admission | One |
| Body, layer, gravity, solver and clearing events.read | One |
| events.read clear:false | None; legacy owner-affinity/retention rules preserved |
| Font registration and UI viewport configuration | One |
| Session, frame, phase and fixed-commit controls | One, conservatively including control-only transitions |
| Readiness registration, next, enter, complete, start, suspend, cancel, release | One, including successful no-op/duplicate acknowledgment |
| Transaction | One if any typed suboperation is mutating; zero if all are reads; zero on rollback |
| Snapshot capture/validate/release | None; receipt lifecycle is independent |

The ABI audit covers all 15 exports. World create issues a new generational identity; destroy invalidates its receipts. gameobject_create/destroy and command_buffer_submit have checked revision barriers after authority/argument checks. transform_set_position and transform_set_rotation_y delegate to command_buffer_submit, sharing its one increment. Successful legacy world_step has one barrier before canonical command/clock/physics mutation. ABI version, error-message reads, statistics, object exports and response copy/free do not mutate content. Error diagnostics are excluded from content and snapshot payloads.

Legacy world_step deliberately preserves its tested partial-commit semantics: after valid admission it drains the batch, retains an accepted prefix, discards the failed tail, and may return an error. That destructive legacy operation advances once, including an error after admission. Invalid time, rational-mode rejection, wrong thread and strict authority rejection do not advance. Strict sessions always reject legacy world_step, so an authenticated snapshot cannot remain current across this legacy partial-commit path. Protocol operations and strict fixed.commit retain atomic staging semantics.

## Host adapter sequence and remaining migration

1. Select the qualified image explicitly in an isolated lane. Leave the active legacy/v3 selection unchanged until its separate integration gate passes.
2. Keep host credentials and phase contexts private. Initialize the same canonical world through its existing phase authority and readiness decisions. Copy/free responses before invoking managed code.
3. Reach initialized Idle, then call snapshot.capture. Retain the owned response or copy its exact bytes. Extract snapshot plus stamp together and verify SHA-256 over the raw snapshot slice. Do not reconstruct identity with local counters or combine this stamp with later mutable world_export_objects data.
4. Bind that exact immutable extraction to the renderer's resolved view identity, viewport/resize generation, color/orientation/layout and requested/submitted/completed capture identities. These are future adapter contracts. They are absent from the native stamp.
5. Validate current identity before work that requires current native state. For asynchronous completion after advancement, use require_current:false and record that the validation is historical. Retain the original extraction until completion. Release the receipt once and free its response independently.
6. Establish retained real managed-session readiness and actual managed mutation/capture correlation in the managed/live-host lane. Native Idle and native receipt retention supply no managed-root readonly or retained CLR lifetime guarantee.

Nested iterator/predicate limitations from READINESS.md remain unchanged. Full renderer/live-session migration, managed-root extraction, presentation provenance, pixel correlation and platform qualification remain pending. The capability name and descriptor deliberately expose those limits.
