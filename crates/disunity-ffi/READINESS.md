# Native lifecycle and readiness primitives, protocol v1

This contract places lifecycle flags and wait decisions in the canonical native world. The bridge executes native-issued managed method or iterator operations. The `native_readiness_nested_start_v1` execution profile also permits bounded immediate lifecycle and routine child scopes while a lifecycle callback or MoveNext is entered. ABI 2 retains its existing 15 exports. Requests use `disunity_world_request`; response ownership and protocol envelopes follow [PHASE_CONTEXT.md](PHASE_CONTEXT.md). Existing legacy entry points and the `native_phase_context_v1` profile retain their behavior until a private caller registers readiness records.

The descriptor advertises `native_readiness_primitives_v1`. Its `native_readiness` object states `managed_integration:false`, `iterator_model:"single_iterator_per_routine"`, `predicates:false`, `nested_stacks:false`, and the limits below. `native_scheduler_v1` and `phase_tokens` remain absent. The existing `native_phase_context` descriptor object is unchanged. Native tests exercise ticket selection and authority; managed Cdecl suites exercise bridge consumption.

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

`readiness.next` issues one outstanding native ticket. It rejects if a ticket is already issued, or if a ticket is entered outside the nested-start profile. With a nested-start entered ticket it selects only that parent's immediate lifecycle children, returning null when none remain. It never selects unrelated queued callbacks or routines through this re-entrant path. `readiness.enter` consumes that exact ticket once before execution. Wrong, stale and duplicate entries reject. On entry, native checks current eligibility again. If intervening gameplay changes invalidate an issued callback or iterator ticket, enter returns `{execute:false}`, revokes the ticket and consumes no lifecycle attempt. The adapter must skip execution and request another decision.

An entered ticket must be completed, including after an application exception. Native lifecycle attempts and once flags are consumed before execution, so exceptions do not cause retries. Callback effects already committed through ordinary native requests survive callback failure. Individual gameplay requests and explicit gameplay transactions retain their existing atomicity. This API does not roll back an entire callback.

Only the **last successful completion in the current root** is retained. Repeating that exact ticket/outcome returns `{}` without effects. A different outcome rejects; older completions reject after another completion replaces the retained pair. This bounded completion acknowledgment never authorizes another execution. No entered or merely issued decision can be replayed through the request ledger.

Every root permits at most 4096 execution claims, counting missing lifecycle methods as native transitions and counting each MoveNext, Current and Dispose separately. The counter resets on a new authenticated root sequence. There is at most one current decision. The nested-start profile may additionally retain entered ancestor tickets in bounded native child scopes. Scripts and retained routine records each have a 4096-record bound. Script metadata remains until world destruction, including cleanup tombstones. Terminal routines remain until explicit release; release requires terminal state and no waiting joiner referencing the routine. Release invalidates the handle permanently. These bounds can reject long-lived worlds that do not retire terminal routines or that register more than 4096 distinct scripts.

phase.finish rejects an outstanding ticket or an unfinished Current/suspend pipeline. A waiting or queued routine may cross normal phase boundaries. Cleanup additionally rejects until all registered awakened scripts have consumed their disable/destroy transitions and every routine has attempted disposal. Terminal session.close/frame.abort still revoke authority without granting cleanup or claiming successful managed disposal. Destroy the world after reporting such infrastructure failure, following the host policy in PHASE_CONTEXT.md.

## Native lifecycle selection

Each next selection scans current native hierarchy activity, component enabled state and pending object/component destruction, including ancestors. Awake requires active hierarchy and ignores the component's enabled bit. OnEnable requires Awake, active hierarchy and enabled component. Start requires enabled eligibility and a FixedUpdate, Update or LateUpdate root. Start and Awake are once-only even if the method is absent or throws. Native schedules all currently eligible Awake transitions before enable transitions, and all currently eligible Start transitions before phase callbacks. A phase callback is attempted at most once per phase root.

OnDisable consumes an enabled-to-ineligible transition. OnDestroy requires an awakened script and terminal owner state, including Cleanup. Missing methods consume their lifecycle transition without returning a decision, using the same claim budget. Root selection uses ascending native component handles within each transition lane. Immediate clone lifecycle selection uses native hierarchy preorder, with native child handle order and component slot order within each object. Every eligible clone Awake is selected before any clone OnEnable. This contract does not implement custom Unity script-execution-order settings.

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

## Immediate lifecycle and routine children

`native_readiness_nested_start_v1` is selected by session.open before gameplay. It extends the existing readiness protocol and capability. It does not introduce another capability version. The existing `native_readiness_nested_start` descriptor advertises immediate routine starts (`immediate_child_starts:true`), immediate clone Awake/OnEnable (`immediate_lifecycle_children:true`), `max_child_depth:32`, `max_claims_per_root:4096`, and `claim_reservation_per_child_scope:3`. The base descriptor's `nested_stacks:false` and the nested descriptor's `yielded_ienumerator:false` describe unsupported yielded IEnumerator stacks. Native child scopes hold suspended execution tickets; they do not implement yielded iterator stacks. The host and bridge both require these fields before selecting the profile.

During an entered lifecycle callback or MoveNext, registration of a new script on an active object associates that script with the exact entered native parent ticket. Identical re-registration does not create another child association or reset flags. Registration outside those entered operations remains deferred. Native checks the existing shared child-depth and claim reservation before admitting immediate registration and again before child selection. Rejected requests preserve the staged world and the parent's ticket.

The bridge binds all clone instances and copies serialized fields before registering and draining their lifecycle. A re-entrant readiness.next selects only Awake and OnEnable transitions for scripts associated with the current parent. Awake ignores the component enabled bit. OnEnable requires active hierarchy and enabled state. Missing methods consume native transitions and claims without issuing managed work. All currently eligible child Awakes run in hierarchy order before child OnEnables. Start remains excluded from immediate selection and is eligible at a subsequent root Start selection after the parent returns.

Each returned lifecycle child decision suspends the entered parent in a native scope. The adapter copies and frees the response, enters the child ticket, invokes the method, records application exceptions, and completes it with ok or failed. Completion consumes the child scope and restores the exact entered parent. An eligibility change before child entry skips that child and also restores the parent. A failed Awake remains consumed, other child callbacks remain eligible, and the invoking parent can continue. Completing or re-entering a suspended parent cannot bypass a child ticket.

StartCoroutine continues using the same child scope stack: native returns an immediate MoveNext decision, then restricts selection to that routine's Current/suspend or Dispose pipeline before restoring the parent. Lifecycle callbacks inside MoveNext may themselves instantiate scripts or start routines. Both kinds of child scope share MAX_CHILD_DEPTH and root claim accounting, including missing lifecycle methods. No STORE or response lock spans managed execution.

Re-entrant bridge Drain consumes only these restricted native decisions. A null decision ends the immediate lifecycle drain with its parent still entered. It does not execute Start, phase callbacks, unrelated lifecycle work, or queued peer routines. The outer root drain handles those after the parent completes. Current and Dispose do not admit new immediate children.

## Adapter boundaries

The bridge keeps context and host credentials private, registers reflection-derived callback metadata, maps instances and iterators to native handles, executes only after `{execute:true}`, reports failures, and drains Cleanup. The native descriptor retains `managed_integration:false`; the selected profile's Cdecl suites exercise its managed adapter. The implementation retains one iterator per routine and provides no yielded IEnumerator push/pop/parent-resume protocol, predicate permission/result protocol, or managed iterator-stack disposal ordering. Renderer, player, browser and full Unity compatibility remain outside this contract.
