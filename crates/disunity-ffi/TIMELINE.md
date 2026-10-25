# Rational timeline and timestamped input S1

This additive native contract uses ABI 2, the existing 15 C exports and version-1 JSON envelopes. It implements exact frame admission and normalized input projections. The native authority remains in `Clock` and `Input`; the existing session owns thread affinity, generational lifetime, staging and retained request identities.

The [phase-context contract](PHASE_CONTEXT.md) adds private phase credentials, host-driven order and pre-script fixed input. Its fixed projection occurs before FixedUpdate; standalone rational timeline worlds project fixed input during physics stepping. The [runtime architecture](../../docs/unity-compatibility-kit-architecture.md) describes managed and player integration.

## Rational values and clock selection

A ratio is `{ "num": unsigned_integer, "den": unsigned_integer }`. Time values use seconds. `time_scale` is dimensionless. Input components must satisfy `num <= 9007199254740991` and `1 <= den <= 1000000000000`, before reduction. Results are reduced, with zero represented as `0/1`. Every intermediate stored result must fit these same bounds. Arithmetic uses checked nonnegative rational operations with bounded u128 intermediates; inexact fallback is forbidden. The 10^12 denominator ceiling supports exact 1/60 plus integer nanoseconds (common denominator 3*10^9). Denominator growth can still reject otherwise small elapsed times.

New owner-thread operations:

```json
{"version":1,"op":"time.set_rational","time_scale":{"num":1,"den":1},"fixed_delta_time":{"num":1,"den":50}}
{"version":1,"op":"frame.begin_rational","dt":{"num":1,"den":60}}
```

`time.set_rational` accepts either, both or neither optional setting. The first rational configure or begin selects rational mode before any legacy frame or physics advancement. Initial rational settings are scale `1/1` and fixed step `1/50`. Prior float configuration is not converted into an exact rational. Later configuration preserves elapsed time, admission totals and remainder.

Frame `dt` is in `[0,1]`, scale is in `[0,100]`, and fixed step is in `(0,1]`. A frame adds `dt * scale` to scaled time and admission remainder, adds `dt` to unscaled time, and admits the exact floor of remainder/fixed-step. Admission removes that exact fixed duration from the remainder. More than 1,024 admissions, invalid ratios, arithmetic bounds and frame overflow reject the request without committing world state. Scaled zero admits no fixed work and preserves remainder. Zero dynamic dt is valid.

Six `1/60` frames at fixed `1/50` admit exactly five steps, with time `1/10` and remainder `0/1`. `frame` advances once per admitted dynamic frame. `fixed_frame` advances once per successful `physics.step`; admission does not execute physics or advance fixed identity. The solver remains float-based. Rational time does not claim exact physical integration.

A selected rational world rejects legacy `frame.begin`, `time.set` and `disunity_world_step`. Legacy-only worlds retain their f32-delta/f64-accumulator/epsilon behavior and queued partial-batch semantics. The C ABI legacy-step rejection preserves pending commands; its established diagnostic path may record the error.

## Clock responses and immutable queries

`time.get`, rational configure and rational begin return existing float API fields plus:

| Field | Meaning |
| --- | --- |
| `unscaled_time` | Session elapsed unscaled seconds, f64 projection |
| `unscaled_delta_time` | Selected API phase's unscaled delta, f32 projection |
| `fixed_unscaled_delta_time` | Fixed delta divided by current scale, f32 projection; zero at scale zero |
| `rational` | Present only in rational mode, exact frame-admission values |

The exact object contains ratios `time`, `unscaled_time`, `delta_time`, `unscaled_delta_time`, `fixed_delta_time`, `time_scale`, `accumulator`, and `admitted_fixed_time`. It obeys `time = admitted_fixed_time + accumulator`, including across fixed-step changes. Exact deltas describe the admitted dynamic frame. `time.phase` selects the existing float `delta_time` and new float `unscaled_delta_time` fields for `fixed` or `update`; it does not change exact admission values or authenticate scheduler access.

Legacy unscaled fields derive from the actual f32 dt supplied to that legacy session. No rational object or exact `1/60` claim is produced from a float input.

`describe.clock_policy` remains `float_accumulated` for existing default-mode consumers and handshake compatibility. New metadata `rational_clock_policy:"explicit_bounded_exact"` describes the opt-in profile. Capabilities are `rational_time`, `unscaled_time`, and `timestamped_input`. Limits expose `max_rational_numerator`, `max_rational_denominator`, `max_fixed_admissions`, and `max_input_events`.

## Normalized ingress

Select rational time before the first event batch. The world handle supplies the generational session identity. OS and replay adapters use identical event records:

```json
{"version":1,"requestId":"ingress-1","op":"input.events","events":[
  {"event_id":1,"timestamp":{"num":1,"den":1000},"source":"os","kind":"key","key":"W","pressed":true},
  {"event_id":2,"timestamp":{"num":2,"den":1000},"source":"os","kind":"key","key":"W","pressed":false}
]}
```

Each event has an unsigned 64-bit `event_id`, rational `timestamp` in session-relative unscaled seconds, and `source:"os"|"replay"`. Use integer-preserving JSON codecs for 64-bit IDs. `event_id` must strictly increase across batches, starting at any value >=1. Gaps are allowed. Exhaustion rejects subsequent ingress until world destruction. Timestamps must be nondecreasing; equal timestamps use event order. After either projection advances, new records must be strictly later than both published cutoffs. Late initial selection also rejects timestamps at or before an already-advanced clock endpoint.

Supported payloads:

| `kind` | Required payload |
| --- | --- |
| `key` | `key:string`, `pressed:bool` |
| `button` | `button:string`, `pressed:bool` |
| `axis` | `axis:string`, `value:f32`, finite in `[-1,1]` |
| `focus` | `focused:bool` |

Unknown fields and mismatched payloads are invalid. Names use the existing 1..128-byte non-control identifier policy. Keys and buttons share that identifier namespace. Per-event held union is bounded at 256 names and per-source analog entries total at 256. Batch size and retained queue size are each 1..4,096 records. These are input budgets, independent of callback limits. The 1 MiB request, 8 MiB response and 16 MiB per-generation outstanding-response limits also apply.

Success returns `{ "accepted_event_ids": [1,2], "queued_events": 2 }`. The complete batch commits atomically, including validation against all intermediate held states. Invalid ingress does not select timestamped mode or consume IDs. Event IDs provide ordering, without a second retry protocol. Identical-byte retries with the existing explicit `requestId` return the retained response and enqueue nothing twice. A replay without that request identity is rejected by event order.

The first valid batch selects timestamped mode. Existing nonempty legacy held/analog or unfocused state prevents selection. Axis definitions with zero projected state can be configured before selection. Once selected, `input.set` and `input.focus` are rejected; focus transitions use normalized events. Legacy-only sessions preserve their existing whole-state replacement, immediate edges, synthetic axis overrides and caller-driven edge lifetime.

## Independent immutable projections

`input.get` accepts optional `phase:"dynamic"|"fixed"`, default `dynamic`. Legacy sessions retain their existing response shape. A normalized snapshot contains:

```json
{"keys":[],"down":["W"],"up":["W"],"axes":{},"focused":true,
 "projection_id":1,"timestamp":{"num":1,"den":100},"last_event_id":2}
```

Rational frame begin advances dynamic projection to its new unscaled endpoint and frame identity. Each successful physics step advances fixed projection to the current admitted frame endpoint and actual fixed identity. Several fixed steps in one frame share this cutoff: the first consumes pending edges, later steps expose held state without repeating edges. Zero fixed steps preserve the fixed cursor. This remains the S1 policy outside strict mode. Strict mode publishes fixed input before FixedUpdate claim, then commits the same snapshot without consuming it again; see PHASE_CONTEXT.md. Subframe input timing remains outside both contracts.

Each consumer applies events at or before its cutoff independently. A press and release within one projection retain both `down` and `up` even when final held state is empty. Repeated presses and releases without a held-state transition add no edge. Reads, enqueue and axis configuration cannot mutate a published snapshot. A subsequent corresponding advancement expires its prior edges. Queue records are reclaimed after both consumers pass them. A paused fixed consumer can fill the bounded queue; ingress then fails without eviction or loss.

Held state unions OS and replay sources. Releasing one source preserves another source's hold. Focus loss is global: it clears both sources, releases all held names and clears analog axes. Unfocused key/axis events are ignored. Focus regain permits a fresh press edge. Keyboard axes use the existing raw native definitions; opposite bindings cancel. Explicit analog values override derived values, with replay overriding OS for the same analog name. Both A and D remain held for gameplay source code that tests D after A. No axis smoothing or gameplay rewrite is added.

`input.configure_axes` affects the next corresponding projection. `input.status` returns `{mode,queued_events,last_event_id,last_timestamp,dynamic_projection,fixed_projection}`, where mode is `legacy` or `timestamped`. A legacy service reports zero IDs, zero counts and `0/1` timestamp.

## Transactions, ownership and remaining integration

All new mutation operations use the established owner-thread and staged commit paths. Atomic transactions include the entire clock, ingress validation head, queue, both projection states, native objects and RNG. Failed transactions discard all of those changes. Existing requestId error retention remains in force. Session ownership and the response ledger remain outside cloned world state.

`time.get`, `input.get` and `input.status` are typed immutable reads. Unretained reads may execute across threads without cloning the whole world. Mutations, retained reads/replays and event ingress require the owner. Response copy/free remain cross-thread and independent of world lifetime. Destroying a world releases clock/input state and creates fresh cursors in a new generation.

The native-only phase-context increment now implements authenticated phase requests and pre-script fixed projection; actual Cdecl consumption still requires foreground integration. Native coroutine deadlines/readiness, imported package update settings, actual OS event forwarding and managed unscaled APIs require later integration. CPU native tests and legacy managed regression runs establish this slice's contracts; they do not establish OS parity, new scheduler callback ordering, historical pixel equivalence or complete game acceptance.
