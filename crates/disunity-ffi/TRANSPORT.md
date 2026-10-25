# Native transport byte limits

## Limits and queries

Every world uses the same byte limits from creation. The native and managed descriptors advertise the baseline `byte_limits` capability and report their bounds in `limits`. The owned-response ABI uses these inclusive limits:

| Resource | Limit |
| --- | --- |
| Request UTF-8 bytes | 1 MiB |
| Serialized reply bytes, including the complete envelope | 8 MiB |
| Outstanding reply bytes | 16 MiB per world generation |
| Live response handles | 1024 process-wide |
| Retained request, ID and reply bytes | 16 MiB per live world |
| Worst-case reply reservation for each new retained ID | 8 MiB |
| Retained identity count | 1024 per live world |

`{"version":1,"op":"transport.get"}` returns immutable effective limits for a live world. This unretained read works across threads and exposes no host key, phase token or mutable counter. It requires an explicit protocol version, rejects request IDs and cannot appear in a transaction. World-zero `describe` replies use a distinct control owner with the same 16 MiB outstanding quota. Invalid world requests return `INVALID_HANDLE` without a response. The typed Rust `request_value` convenience API returns transient values without creating response handles or outstanding storage.

Retained request storage is separate from outstanding response storage. A new retained ID must have room for its request bytes, ID and an 8 MiB reply reservation before execution. This conservative reservation can reject before the count ceiling. Replays require fresh outstanding admission and never reexecute gameplay.

## Ownership and atomic admission

Each response records its generation-checked world owner or the control owner. Outstanding bytes are isolated between owners. World destruction does not free or invalidate owned response bytes. A reused world slot receives a new generation and a fresh quota. Freeing an old reply releases its original owner only. Immutable copies and single-use frees work across threads.

All owners share the process-wide 1024-handle ceiling. Combined with the 8 MiB reply limit, this bounds logical owned-response bytes at 8 GiB. These bounds exclude temporary copies, vector capacity and other native storage.

The response pool lock precedes the world store lock. Session code supplies the resolved immutable owner directly to admission; admission does not reacquire the world store. Parsed mutations use one staged world. Bounded serialization of the complete envelope and exact owner byte/count admission finish before staged world, RNG, input, transforms, lifecycle/time state, retained snapshot metadata or retained identity commit.

Outstanding quota rejection returns C ABI `BUDGET` with response zero and leaves a new request ID retryable. A serialized reply over 8 MiB produces the bounded deterministic rollback error. When admitted, that semantic error follows the retained-error replay contract. Malformed JSON returns `INVALID_ARGUMENT` without a response. Oversized requests return `BUDGET` before parsing or reading the claimed request buffer.

## Serialization and allocation bounds

The JSON writer checks remaining byte capacity on every write and stops before extending the response vector beyond 8 MiB. Transaction aggregate counting uses the same limit and stops before later suboperations. Exact final serialization checks the complete response envelope before commit.

Runtime and C# transport consumers always bound request encoding to 1 MiB and probe replies before allocating their copy buffers. Native and managed replies are capped at 8 MiB, including managed observation and diagnostic responses. Copy failures still release owned responses; primary and release failures remain observable together.

Gameplay operations construct JSON values and stage world data before writing. Snapshot capture computes its canonical digest from separate snapshot bytes under the represented-snapshot budget. Rust allocator failure during general world/value construction retains the platform allocation-failure behavior. These limits bound logical transported request, reply and outstanding bytes. Total native heap use and general allocator-failure recovery are outside their scope.
