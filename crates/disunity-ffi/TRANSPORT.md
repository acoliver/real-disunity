# Native transport byte profile

## Selection and effective limits

The owned-response C ABI supports an explicit per-world-generation byte profile. Create a world, then send:

```json
{"version":1,"op":"transport.select","transport_profile":"transport_byte_limits_v1"}
```

Selection requires the world's creation thread and an untouched world: no committed content mutation, strict session, retained request, or previous selection. Select before `session.open` when using strict native sessions. Selection has no request ID, cannot appear in a transaction, and cannot be downgraded or repeated. Unsupported profiles, fields, protocol versions, or retained transport controls are rejected. A failed transport control does not reserve its request ID. Selection commits only after admitting its reply to the selected target bucket. It does not advance gameplay content revision or change snapshot contents.

Query a live world with `{"version":1,"op":"transport.get"}`. This unretained read works across threads and returns public effective limits. It exposes no host key, phase token, or mutable transport counter. World-zero `describe` remains a global legacy-pool operation. The typed Rust `request_value` convenience API returns transient values without creating response handles or outstanding storage.

| Limit | Default legacy | Explicit target |
| --- | --- | --- |
| Request UTF-8 bytes | 1 MiB | 1 MiB |
| Serialized reply bytes | 16 MiB | 8 MiB |
| Outstanding reply bytes | 64 MiB process legacy pool | 16 MiB per world generation |
| Live response handles | 1024 process-wide, shared across profiles | Same shared 1024 handles |
| Retained request + ID + reply bytes | 32 MiB per live world | 16 MiB per live world |
| Worst-case reply reservation for each new retained ID | 16 MiB | 8 MiB |
| Retained identity count ceiling | 1024 | 1024 |

All byte limits are inclusive. Retained request storage is separate from outstanding response storage. A new retained ID must have room for its request bytes, ID, and the profile's maximum reply before execution. This conservative reservation can reject before the retained count ceiling. Replays require fresh outstanding admission and never reexecute gameplay. Legacy limits and its conservative 16 MiB outstanding preflight are unchanged, including full-pool error precedence.

## Ownership and atomic admission

A response records the profile and generation-checked world identity that admitted it. Outstanding target bytes are isolated from other target generations and from the legacy byte pool. Existing legacy replies stay in their original bucket when an untouched world selects the target. The selection reply itself belongs to the target bucket.

World destruction does not free, migrate, or invalidate owned response bytes. A reused world slot receives a new generation and a fresh quota. Freeing an old reply releases its original owner only. Immutable copies and single-use frees work across threads. The process-wide 1024-handle ceiling can still exclude a new world when other worlds retain all handles. Target replies have an additional conservative process byte ceiling of 1024 times 8 MiB, or 8 GiB; there is no shared target 64 MiB byte pool. The legacy pool remains capped at 64 MiB. These ceilings describe logical owned-response bytes, excluding temporary copies, vector capacity and other native storage.

The response pool lock precedes the world store lock. Session code supplies the resolved immutable owner directly to admission; admission does not reacquire the world store. Parsed mutations use one staged world. Bounded serialization and exact owner byte/count admission finish before staged world, RNG, input, transforms, lifecycle/time state, retained snapshot metadata, or retained identity commit. Transport quota rejection returns C ABI `BUDGET` with response zero and leaves a new request ID retryable. A serialized reply over the profile limit produces the bounded deterministic rollback error. When admitted, that semantic error follows the existing retained-error replay contract. Malformed JSON returns `INVALID_ARGUMENT` without a response. Oversized requests return `BUDGET` before parsing or reading the claimed request buffer.

## Serialization and allocation bounds

The JSON writer checks remaining byte capacity on every write and stops before extending the response vector beyond the effective reply limit. Transaction aggregate counting uses that same effective limit and stops before later suboperations. Exact final serialization also checks the complete response envelope.

Gameplay operations still construct JSON values and stage world data before writing. Snapshot capture also computes its canonical digest from separate snapshot bytes under the existing represented-snapshot budget. Rust allocator failure during general world/value construction retains the platform's allocation-failure behavior. This profile bounds logical transported request/reply/outstanding bytes; total native heap use and general allocator-failure recovery are outside its bounds.

`describe.capabilities` adds `transport_byte_limits_v1`; the existing `limits` descriptor remains the legacy descriptor. The new descriptor explicitly reports `managed_integration: false`. The [runtime byte-profile consumer](../disunity-runtime/src/compatibility/transport/byte_profile.rs) negotiates the selected limits with the managed bridge. The byte profile retains the existing ABI exports and interface revision.
