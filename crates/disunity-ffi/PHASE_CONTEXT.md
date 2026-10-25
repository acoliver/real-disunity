# Native host-driven phase contexts, protocol v1

This opt-in contract uses ABI 2 and the existing 15 exports. It implements host authorization, native frame order and one outstanding linear phase lease in the canonical STORE. Descriptor capability is `native_phase_context_native_v1`; the selected profile is `native_phase_context_v1`. [Readiness](READINESS.md) adds lifecycle and iterator tickets. The [managed runtime](../disunity-runtime/src/compatibility/phase.rs) transports private contexts to the Cdecl bridge. `phase_tokens` and `native_scheduler_v1` are absent from the native descriptor.

## Transport and credentials

Requests use `disunity_world_request(actual_world_handle,...)`. Existing JSON response envelopes remain `{version:1,ok:true,data:...}` or `{version:1,ok:false,error:{code,message}}`. Include `version:1`; the existing unversioned operation compatibility also applies. All new control operations reject requestId and transaction nesting. No successful claim response can be replayed through the exact-byte ledger. Duplicate finish also rejects. Existing normal-operation deduplication and atomic transaction behavior remain intact.

Unknown fields are rejected in request, host, context and ratio DTOs. Host is `{session:string,host_key:string}`. Context is exactly `{world:u64,session:string,frame:u64,fixed_tick:u64,phase:Phase,sequence:u64,token:string}`. Session, host_key and token are native-issued 64-character lowercase hexadecimal strings. Session identity is public. The host key and phase token each contain 256 independent OS-random bits. Linux `/dev/urandom` is read through std::fs outside STORE and RESPONSES locks, with checked read failure before mutation. No new dependency, gameplay RNG or global authentication state is involved. Unsupported entropy sources fail closed. This entropy path requires Linux `/dev/urandom`.

Keep host_key in private host state. Keep context only in the private host/bridge invocation envelope. Neither belongs in script arguments, public NativeWorld state, snapshots, descriptions, journals, logs or rendering metadata. Responses carrying credentials must be copied privately and freed. Native types holding credentials have no Debug implementation. Ordinary fully privileged in-process code can still inspect process memory; this is an API misuse boundary rather than a process sandbox.

## Operations

Fields below exclude common `version` and `op`.

| op | Input | Successful data and behavior |
| --- | --- | --- |
| session.open | `{profile:"native_phase_context_v1"}` | `{session,host_key,profile,supported_phases}`. One-time owner-thread selection before any frame/fixed advancement, with no active legacy lifetime. Selects the rational clock without advancing it. Existing world objects/configuration can remain. No downgrade/reopen. |
| session.status | `{}` | Credential-free Status below. Immutable unretained cross-thread read. Legacy world returns `{profile:"legacy"}`. |
| session.configure | `{host,time_scale?:Ratio,fixed_delta_time?:Ratio}` | Time DTO. Allowed before Initialize or in Idle, with no active lease. Same exact rational validation as S1. |
| frame.open | `{host,dt:Ratio}` | `{time:Time,stamp:Status}`. Requires initialized Idle. Native admits exact fixed count, advances dynamic projection and begins lifetime, atomically. |
| phase.open | `{host,phase:Phase,frame:u64,fixed_tick:u64}` | `{context:Context,stamp:Status}`. Asserts the next native stage and identities, creates one Issued lease. Sets the API time projection. FixedUpdate prepares fixed input **before claim/callback/solver**, with projection_id equal to the pending fixed_tick. Cleanup is the terminal lane described below. |
| phase.claim | `{phase:Phase,context:Context}` | `{execute:true,stamp:Status}`. Consumes Issued exactly once before user execution. Checks actual ABI world generation, native session, full context identity, outer phase, owner thread and non-duplicate claim. No credentials or claim can be fabricated from status. |
| phase.finish | `{host,context:Context}` | Status. Requires the current Claimed root, revokes it and advances to the next native stage. Call in host finally after bridge return, including ordinary application callback failure. Cleanup finishes to Closed. Duplicate or unclaimed finish rejects. |
| phase.cancel | `{host,context:Context}` | Status. Only the current **unclaimed** lease can cancel. Revokes its token and permits a fresh phase.open at the same stage. Restores the originating stage and pre-open time projection, including Cleanup cancellation back to Idle or pre-Initialize. FixedUpdate additionally restores the entire pre-projection input state/queue; no lost edges. Claimed work cannot be cancelled this way. |
| fixed.commit | `{host,frame:u64,fixed_tick:u64}` | `{time:Time,stamp:Status}`. Only after FixedCoroutines finishes. Uses native admitted fixed_delta_time, applies pending ABI body commands, solves, creates contacts and increments actual fixed_frame exactly once. No caller dt override is accepted. Preserves the already-published fixed snapshot without consuming its edges again. |
| frame.end | `{host,frame:u64}` | Status. Only after EndOfFrame finishes. Flushes deferred destruction and returns Idle, atomically. |
| frame.abort | `{host,frame:u64}` | Terminal Closed Status. Revokes the root and restores an outstanding uncommitted fixed projection/checkpoint. Already admitted frame time, committed solver steps and gameplay effects are retained; this is terminal failure, never a successful frame or transaction rollback. This grants no cleanup root; destroy this world after reporting any failed cleanup. |
| session.close | `{host}` | Same terminal revocation as abort, usable at any stage. Does not claim to dispose managed roots or drain native scheduler records. No reopen or gameplay writes after close. For normal unload/dispose, finish a claimed Cleanup root before destroying the world. The separately selected [terminal cleanup contract](TERMINAL_CLEANUP.md) adds recovery authority for open strict sessions. |

Host must not retry a claim after an uncertain successful transport result. A successful claim means one permitted bridge execution. Transport recovery uses private saved response bytes where available, or terminal abort/close; it never executes again by resubmitting a linear control operation.

## Native sequence and counters

Phase enum is exactly `Initialize`, `FixedUpdate`, `FixedCoroutines`, `Contacts`, `Update`, `UpdateCoroutines`, `LateUpdate`, `EndOfFrame`, `Cleanup`. The coroutine-named lanes are ordering slots for the host's existing scheduler. They carry no native coroutine readiness claim.

1. Open/claim/finish Initialize once at frame=0, fixed_tick=0.
2. frame.open admits N native steps. For each: FixedUpdate, FixedCoroutines, fixed.commit, Contacts. Open/claim/finish each phase even when the bridge has no eligible work.
3. Update, UpdateCoroutines, LateUpdate, EndOfFrame, then frame.end.
4. Repeat from Idle. Zero-admission/paused frames skip the complete fixed loop and leave fixed input unchanged.
5. For unload/dispose at Idle, open/claim Cleanup using current frame and committed fixed_tick, run managed cleanup, then finish to Closed. A failed setup may use the same lane before Initialize issuance, or after cancelling an unclaimed Initialize. No active frame or outstanding root may be bypassed.

FixedUpdate/FixedCoroutines assert `fixed_tick = actual fixed_frame + 1`. Contacts and dynamic phases assert the committed fixed_frame. Contacts retains fixed delta projection; Update switches to the admitted dynamic delta. All phases assert current frame. Counters are native assertions, never caller assignments. At most one root is outstanding; frame.open, phase.open, commit/configuration/end cannot reenter an issued or claimed root. Sequence/revision/frame/tick arithmetic is checked; claims are bounded at 4096 per frame, active contexts at one, fixed admissions at 1024. No historical credential collection grows with frame count. Existing transaction limit remains 128 operations and ingress batch/queue limit remains 4096.

Gameplay writes are allowed before Initialize issuance and while a root is Claimed. Writes during Issued leases, idle post-initialization gaps or Closed reject. Normal callback transactions remain staged and rollback atomically. A strict control cannot be nested in a transaction. Input ingress is permitted only before Initialize or in Idle without a lease, and should be enqueued before frame.open. All actual native requests enforce these restrictions, including requests bypassing managed facades.

Strict worlds reject bare frame.begin/frame.begin_rational, physics.step, time.set/time.set_rational/time.phase, lifetime.begin/end and ABI world_step. Ordinary legacy worlds keep their original endpoints, float clocks, input policy, deduplication and partial command-prefix behavior. ABI object/command writes also obey root-stage restrictions. Pure queries remain available; owner affinity still applies to mutations and retained reads. `events.read {clear:false}` is allowed before Contacts claim and never drains the queue. It retains the legacy owner-affinity and response-ledger policies. Omitted clear or clear:true still requires a Claimed root.

## Terminal Cleanup lane

Cleanup uses existing operations and Context fields without changing the ABI or protocol version:

```
phase.open   {host, phase:"Cleanup", frame:<status.frame>, fixed_tick:<status.fixed_tick>}
             -> {context, stamp}  // stamp.stage = {"Phase":"Cleanup"}
phase.claim  {phase:"Cleanup", context:<exact returned context>}
             -> {execute:true, stamp}
phase.finish {host, context:<same context>}
             -> Status           // stage = "Closed", active_phase = null
```

Issuance requires Idle or the pre-Initialize stage with no active root. It neither opens a frame nor changes time/input projections, admits fixed work, begins a lifetime, or resets counters. Wrong credentials/frame/tick, issued or claimed roots, active frame stages, Commit, End, and Closed reject without effects. Cleanup uses the same independent OS token, checked sequence/revision and single-claim budget as other phases. Its issued context authorizes no gameplay writes. Its claimed context permits the existing gameplay mutation API, including atomic callback transactions. Native advancement and input ingress remain forbidden.

Unclaimed cancellation restores the originating stage and revokes the token. Claimed Cleanup cannot cancel. Finishing after callbacks, including callback errors, revokes the token and closes permanently. Repeated claim/finish, requestId replay, nested controls, post-close writes and reopen reject. session.close can still terminally revoke a failed Cleanup root and grants no further cleanup authority. Native authority covers the root only; managed lifecycle eligibility, callback selection and iterator disposal remain owned by the adapter.

## Exact desktop-compatible timebase

Ratio is `{num:u64,den:u64}`, nonnegative, with num <= 9007199254740991 and 1 <= den <= **1000000000000**, including normalized arithmetic results. This permits exact rational frame durations combined with integer nanosecond input timestamps.

Bounded u128 intermediates preserve reduced exact sums/products. `1/60 + 1/1000000000 = 50000003/3000000000` is accepted without rounding. Integer nanoseconds plus exact 1/60 subdivisions share a denominator of 3000000000. Unrelated co-prime denominators or scale products may still exceed the explicit 10^12 or numerator bounds and reject atomically. Solver duration remains the existing f32 projection of the admitted exact duration; no exact physics integration claim is made.

Legacy S1 fixed input remains post-physics outside strict mode. Strict mode advances it at FixedUpdate open, using that admitted frame's unscaled endpoint, before scripts. Repeated fixed steps at the same endpoint get distinct pending tick IDs; the first consumes pending edges and later steps see held state without repeated edges. fixed.commit retains the same snapshot. This is endpoint sampling, with no subframe timing fidelity claim.

## Safe native observation stamps

Status contains exactly `{profile,world,world_generation,session,frame,fixed_tick,pending_fixed_tick,stage,revision,revision_kind,sequence,claims,remaining_fixed,active_phase}`. `stage` is `{"Phase":Phase}` or the string `Idle`, `Commit`, `End`, `Closed`. pending_fixed_tick is null except while a prepared fixed tick remains uncommitted. active_phase is null or `{phase,sequence,claimed}`. No host key or dispatch token is present.

revision_kind is `schedule_transition`. Revision advances for native schedule/configuration/lease transitions, **not every world mutation**. It must not be treated as a content revision or render-data hash. Native snapshot includes `session_stamp` from the same locked read as its objects/time/input, establishing the observed schedule identity without inventing a global world-state revision. The [snapshot contract](SNAPSHOTS.md) adds a separate content revision and retained immutable extraction. Render and presentation metadata belong to the consumer.

## Host and bridge consumption

[CompatibilityHost](../disunity-runtime/src/compatibility.rs) negotiates native and managed descriptors before loading scripts. For strict worlds it opens Initialize, passes the private context to the bridge, and finishes the root after bridge return. The bridge claims the context before binding or invoking scripts. The [phase-context tests](../disunity-runtime/tests/phase_contexts.rs) exercise actual Cdecl consumption and rejection of invalid contexts.

Copy and free native responses before managed execution. Neither STORE nor RESPONSES locks span callbacks. Dispatch only after `{execute:true}` and finish the claimed root after the bridge returns. Application callback errors remain observable while the host completes the native sequence. Infrastructure failures use the selected cleanup contract or terminal close; an uncertain successful claim is never retried as execution.

Normal unload opens Cleanup at Idle or before Initialize, claims it before OnDisable/OnDestroy/Dispose, and finishes to Closed. [Terminal cleanup](TERMINAL_CLEANUP.md) supplies a dedicated recovery root when an open strict session fails during active work. Closed sessions cannot reopen. Native phase authority, native readiness obligations and measured managed roots remain separate contracts.
