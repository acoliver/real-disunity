# Native lifecycle and readiness primitives, protocol v1

This contract moves lifecycle flags and wait decisions into the canonical native world. It provides a bounded API for a future bridge adapter to execute one managed method or iterator operation at a time. ABI 2 retains its existing 15 exports. Requests use `disunity_world_request`; response ownership and protocol envelopes follow [PHASE_CONTEXT.md](PHASE_CONTEXT.md). Existing legacy entry points and the `native_phase_context_v1` profile retain their behavior until a private caller registers readiness records.

The descriptor advertises `native_readiness_primitives_v1`. Its `native_readiness` object states `qualification:"native_only_bridge_integration_pending"`, `managed_integration:false`, `iterator_model:"single_iterator_per_routine"`, `predicates:false`, `nested_stacks:false`, and the limits below. `native_scheduler_v1` and `phase_tokens` remain absent. The existing `native_phase_context` descriptor object is unchanged. Native-only tests do not qualify managed Cdecl consumption of this API.

## Private authorization

Every operation in this document requires `context`, exactly the native-issued context from phase.open, after phase.claim succeeds. The context is `{world:u64,session:string,frame:u64,fixed_tick:u64,phase:Phase,sequence:u64,token:string}`. Validation compares every field against the current claimed lease and the actual generation-checked ABI world handle. Native creates the frame, tick and stage identities. Session status supplies no token. The token must remain in the bridge's private invocation envelope, separate from public NativeWorld state and script arguments.

All readiness operations require the world's owner thread and reject requestId, transaction nesting and unknown fields. They also reject legacy worlds, unclaimed roots, revoked contexts and closed sessions. Status and snapshot remain unretained cross-thread reads and contain no readiness credentials. Requests are staged by cloning the existing world under the existing STORE. A rejected request or response-budget failure commits no readiness, clock, RNG, input, lifecycle or gameplay state. Generation allocation may leave unused identities after a rejected staged operation; identities are never reused.

Native never invokes user code. Copy and free the native decision response before calling managed code. No STORE or RESPONSES lock spans a managed callback. This is an in-process API authorization boundary; privileged native or managed code can inspect process memory.

## Wire schema

All rows also accept common `version:1` and require `context:Context`. The existing protocol's unversioned compatibility applies. Successful envelopes are `{version:1,ok:true,data:...}`; rejected requests use the existing `invalid_request` error envelope.

| op | Additional fields | Successful data |
| --- | --- | --- |
| readiness.register | `component:u64, callbacks:Callback[]` | `{}` |
| readiness.next | none | `Decision` or `null` |
| readiness.enter | `ticket:u64` | `{execute:bool}` |
| readiness.complete | `ticket:u64, outcome:Outcome` | `{}` |
| readiness.start | `component:u64` | `{routine:u64}` |
| readiness.suspend | `routine:u64, wait:Wait|null` | `{}` |
| readiness.cancel | `routine:u64` | `{}` |
| readiness.release | `routine:u64` | `{}` |

`Callback` is exactly one of `Awake`, `OnEnable`, `Start`, `FixedUpdate`, `Update`, `LateUpdate`, `OnDisable`, `OnDestroy`. Registration requires an actual live Script component, including its generation, and rejects pending destruction, unknown handles and other component kinds. Metadata is immutable: identical re-registration succeeds without resetting flags; changing the callback set rejects. Duplicate callback names and more than eight entries reject. New Script components may be created and registered inside a claimed callback.

`Decision` has exactly `{ticket:u64,kind:string,component:u64,routine:u64|null}`. `kind` is a Callback name, `MoveNext`, `Current` or `Dispose`. A lifecycle decision has `routine:null`. Routine and ticket IDs carry process-unique native generations, are scoped to their owning world and are never caller-assigned.

`Outcome` is `ok`, `failed`, `more` or `done`, with the following permitted combinations:

| Decision kind | Outcomes | Native transition |
| --- | --- | --- |
| lifecycle Callback | ok / failed | Attempt already consumed at enter. |
| MoveNext | more | Permit one Current claim. |
| MoveNext | done / failed | Require Dispose. |
| Current | ok | Require suspend, or cancellation. |
| Current | failed | Require Dispose. |
| Dispose | ok / failed | Terminal. Joins can subsequently resume. |

## Linear tickets and phase boundaries

`readiness.next` issues one outstanding native ticket. It rejects if any ticket is already issued or entered. `readiness.enter` consumes that exact ticket once before execution. Wrong, stale and duplicate entries reject. On entry, native checks current eligibility again. If intervening gameplay changes invalidate an issued callback or iterator ticket, enter returns `{execute:false}`, revokes the ticket and consumes no lifecycle attempt. The adapter must skip execution and request another decision.

An entered ticket must be completed, including after an application exception. Native lifecycle attempts and once flags are consumed before execution, so exceptions do not cause retries. Callback effects already committed through ordinary native requests survive callback failure. Individual gameplay requests and explicit gameplay transactions retain their existing atomicity. This API does not roll back an entire callback.

Only the **last successful completion in the current root** is retained. Repeating that exact ticket/outcome returns `{}` without effects. A different outcome rejects; older completions reject after another completion replaces the retained pair. This bounded completion acknowledgment never authorizes another execution. No entered or merely issued decision can be replayed through the request ledger.

Every root permits at most 4096 execution claims, counting missing lifecycle methods as native transitions and counting each MoveNext, Current and Dispose separately. The counter resets on a new authenticated root sequence. There is at most one outstanding decision. Scripts and retained routine records each have a 4096-record bound. Script metadata remains until world destruction, including cleanup tombstones. Terminal routines remain until explicit release; release requires terminal state and no waiting joiner referencing the routine. Release invalidates the handle permanently. These bounds can reject long-lived worlds that do not retire terminal routines or that register more than 4096 distinct scripts.

phase.finish rejects an outstanding ticket or an unfinished Current/suspend pipeline. A waiting or queued routine may cross normal phase boundaries. Cleanup additionally rejects until all registered awakened scripts have consumed their disable/destroy transitions and every routine has attempted disposal. Terminal session.close/frame.abort still revoke authority without granting cleanup or claiming successful managed disposal. Destroy the world after reporting such infrastructure failure, following the host policy in PHASE_CONTEXT.md.

## Native lifecycle selection

Each next selection scans current native hierarchy activity, component enabled state and pending object/component destruction, including ancestors. Awake requires active hierarchy and ignores the component's enabled bit. OnEnable requires Awake, active hierarchy and enabled component. Start requires enabled eligibility and a FixedUpdate, Update or LateUpdate root. Start and Awake are once-only even if the method is absent or throws. Native schedules all currently eligible Awake transitions before enable transitions, and all currently eligible Start transitions before phase callbacks. A phase callback is attempted at most once per phase root.

OnDisable consumes an enabled-to-ineligible transition. OnDestroy requires an awakened script and terminal owner state, including Cleanup. Missing methods consume their lifecycle transition without returning a decision, using the same claim budget. Selection order uses ascending native component handles within each transition lane; this contract makes no Unity script-execution-order compatibility claim.

Routine disposal runs after Awake/enable/disable reconciliation and before OnDestroy, Start or peer phase callbacks. Each later claim rechecks eligibility, including effects made by Dispose. This preserves mutation-sensitive peer suppression. A script callback may register new scripts; the next scan includes them. Metadata for a removed generation remains a cleanup tombstone and cannot authorize new registration or routine start.

## Routine state and cancellation

Start requires a registered, live Script owner in an active hierarchy, outside Cleanup and pending destruction. Disabled Script components may own routines. Start creates a Ready routine without executing managed code. The adapter retains the CLR iterator separately, indexed by the native routine handle. Next returns MoveNext; a successful `more` report permits Current exactly once. Successful Current requires a strict wait descriptor. Suspend before Current, duplicate suspend or invalid outcomes reject.

Owner inactivity, removal, pending destruction or Cleanup requests cancellation. Gameplay mutation effects latch cancellation in native state, including ancestor changes, so reactivation cannot revive an old routine. Cancellation effects inside a failed gameplay transaction roll back with the transaction. Disabling a Script alone keeps its routine. Explicit cancel is idempotent for a known terminal routine; unknown/released/cross-world handles reject. Cancellation during an entered operation allows its completion report and suppresses further MoveNext/Current, then requires Dispose. Dispose failures count as attempted terminal disposal and permit joins, matching the existing managed scheduler's completion-before-cleanup-error behavior.

## Strict waits and native time

Wait is null or one of these exact tagged objects. Omitted optional wait also selects next. Caller frame, tick, deadline or predicate fields reject. Ratio is exactly `{num:u64,den:u64}`, with the existing numerator bound and denominator in 1..=1000000000000. Native normalizes inputs and rejects arithmetic whose normalized result exceeds those bounds.

| Wait | Readiness rule |
| --- | --- |
| null / `{kind:"next"}` | UpdateCoroutines in a subsequent dynamic frame. |
| `{kind:"scaled",seconds:Ratio}` | Subsequent dynamic frame's UpdateCoroutines, exact native scaled endpoint at or beyond the native-computed deadline. |
| `{kind:"unscaled",seconds:Ratio}` | Same frame gate, using the native unscaled endpoint. |
| `{kind:"fixed"}` | FixedCoroutines at the next admitted tick. A wait registered in FixedUpdate can resume at that pending tick; a wait registered in FixedCoroutines cannot resume again in the same tick. Contacts and later lanes target the next tick after the committed one. |
| `{kind:"end_frame"}` | EndOfFrame in the same frame when registered earlier; a wait registered inside EndOfFrame targets a subsequent frame. |
| `{kind:"join",routine:u64}` | Subsequent dynamic frame's UpdateCoroutines after target terminal disposal. Reject unknown/released/cross-world handles, self-joins and indirect cycles. |

Deadlines use the existing rational dynamic and unscaled Timeline endpoints. No host-computed absolute deadline, float tolerance, extra clock, external time source or duplicate scheduler exists. Pausing scaled time preserves scaled waits while unscaled waits progress. Zero-second waits still require a subsequent frame. Fixed waits consume no tick until native frame admission and fixed lane progression occur. EndOfFrame readiness uses its dedicated native phase.

## Adapter work remaining

The implementation owns one iterator record per routine. It provides no nested IEnumerator push/pop/parent-resume protocol, predicate permission/result protocol or managed stack disposal ordering. It also serializes tickets: a synchronous StartCoroutine invoked inside an entered callback queues the new native routine until that callback completes. The existing managed scheduler's immediate nested start behavior therefore needs an explicit future protocol before compatibility activation.

A future adapter must keep context and host credentials private, register reflection-derived callback metadata, map script instances and iterators to actual native handles, execute only after `{execute:true}`, report failures in finally, and drain Cleanup. It must qualify actual Cdecl rejection of forged contexts and duplicate entry. Legacy CoroutineScheduler continues owning its timers and lifecycle state until that adapter explicitly selects a separately qualified profile. Renderer, player, browser, predicate, nested-stack and full-game qualification remain outside this native slice. See the [session handoff](../../.session/resumed-native-readiness.md) for provider identity and actual verification results.
