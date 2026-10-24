# Native terminal cleanup contract

## Provider and scope

The additive capability is `native_terminal_cleanup_v1`, with ABI 2, protocol 1 and the existing 15 exports. The descriptor's `native_terminal_cleanup` object reports `managed_integration:false`, `closed_recovery:false`, `projection_policy:"freeze_current"`, `recovery_roots:1`, `readiness_ticket_reconciliation:true` and `max_readiness_claims:4096`. Existing phase, readiness and snapshot capabilities remain independent. This provider supplies native authority and native attempt accounting. CLR execution, managed ownership and runtime dispatch-depth enforcement require a matching consumer.

## Wire operation

`session.begin_cleanup {host, reason?}` returns `{context, stamp}`. The private host object is the exact `{session,host_key}` from `session.open`. Optional reasons are `infrastructure_failure` and `host_shutdown`; absent or null reason has identical transition semantics. Unknown fields and reasons reject. The operation rejects requestId and transaction nesting. No caller may supply frame, fixed tick, sequence or credentials for the new context.

The returned context retains the existing shape: `{world,session,frame,fixed_tick,phase:"Cleanup",sequence,token}`. It is newly issued native authority. Every numeric field is an unsigned integer, including full-width u64 identity and sequence fields. Only the exact owner-thread claim of this context authorizes Cleanup. Duplicate claims, forged fields, foreign world/session/generation and stale contexts reject.

Status adds `terminal_cleanup:bool` and `terminal_recovery`, null outside terminal mode. Terminal recovery metadata contains:

- `claims`: zero before claim and one after claim. Gameplay `claims` is unchanged by this recovery claim.
- `abandoned_fixed_tick`: an admitted uncommitted fixed projection's identity, or null. It is diagnostic metadata and never a committed tick.
- `execution_unknown`: the count of entered old tickets whose completion was abandoned. At most one ticket can be outstanding. This records uncertainty about execution and never certifies callback success.
- `native_obligations`: `remaining_scripts`, `remaining_routines`, `claims`, `max_claims`, `attempts_drained`, and `outstanding_ticket`. Script counts represent records with remaining lifecycle obligations; a record may need both OnDisable and OnDestroy. These counts describe native records, with no CLR root-count assertion.

Host keys and phase tokens occur only in private credential responses and requests. Status, errors, snapshots and Debug implementations expose neither credential. Retain private owned response bytes when transport receipt is uncertain. Copying the response does not execute the operation again.

## Admission and permanent terminality

An open strict world may transition from preinitialization, Idle, any issued or claimed gameplay root, Commit, End, or an ordinary unclaimed Cleanup root. An ordinary claimed Cleanup root, existing terminal transition, legacy world or Closed session rejects. Closed never reopens. A repeated request cannot obtain another recovery allowance.

Admission authenticates the private host on the owner thread. Native entropy is generated during request preparation outside the world lock. Owned-response capacity is reserved before world execution. The existing canonical clone stages the checked content revision, sequence and schedule revision, old-ticket reconciliation, authority revocation and final response serialization. Failure commits no staged world changes or RNG draws. There is no checkpoint recovery or second world model.

The terminal root has one dedicated claim allowance, independent of the 4096 gameplay-root limit. No gameplay counter is reset. Terminal phase.cancel rejects. Explicit session.close or frame.abort permanently revokes authority and can leave incomplete native obligations. Their response must never be interpreted as successful managed teardown.

## Frozen state and cleanup mutations

Admission preserves the actual current clock, API delta, both input snapshots, frame, committed fixed tick, admitted fixed endpoint, accepted mutations, RNG state and queued body intent. Pending fixed checkpoints are discarded without restoration. `pending_fixed_tick` becomes null and `remaining_fixed` becomes zero. No Initialize, zero-dt frame, solver, fixed commit, input advance or frame end runs. Cleanup input consumers use the existing dynamic-lane projection; the preserved fixed projection can describe abandoned precommit work.

Until claim, reads are allowed and gameplay mutations reject. After claim, existing object/component cleanup writes, destruction and atomic transactions remain available. Native object creation and non-Script component attachment remain supported. New Script components, object cloning, readiness registration and routine starts reject. All object cloning is conservatively rejected because it can copy Script descendants. Input ingress/configuration/focus, time changes, frame advancement and physics operations reject. Existing read-only queries and nonclearing event reads remain available. Ordinary authorized cleanup mutations, including RNG draws, retain their established semantics; admission itself consumes no gameplay RNG.

Every successful content-mutating outer operation advances content_revision once. Transaction suboperations share that outer increment. Failed requests, failed transactions and failed response admission advance neither content nor schedule revision. Begin cleanup advances the schedule revision once. Retained snapshot current validation becomes stale; historical retained identity and previously owned snapshot bytes remain unchanged. Existing snapshot authentication rules still reject validation after Closed.

## Readiness reconciliation and once-only attempts

The transition revokes the old root, outstanding ticket and last-completion acknowledgment. It preserves script metadata tombstones, once flags and terminal routines. It changes scheduler eligibility without modifying hierarchy active/enabled flags.

- Issued MoveNext or Current is abandoned and its routine enters Disposing.
- Entered MoveNext or Current is execution-unknown, is never reentered, and its routine enters Disposing.
- Completed Current awaiting suspend and waiting/joining routines enter Disposing.
- Issued Dispose is revoked and can receive one fresh cleanup authorization because execution had not entered.
- Entered Dispose becomes a terminal uncertain attempt. Another Dispose is never authorized.
- Entered lifecycle tickets retain their consumed flags. Completed OnDisable/OnDestroy attempts remain consumed. Unentered cleanup lifecycle tickets may be freshly selected.
- Already terminal or released routines stay terminal or absent. Dead/pending owners retain metadata needed for cleanup, with no handle resurrection.

The selected scheduler must continue using readiness.next, enter and complete under the fresh claimed context. Terminal mode offers OnDisable, Dispose and OnDestroy work. It offers no Awake, Start, MoveNext, Current or gameplay update. Failed callback or Dispose completion consumes its attempt and permits peer progress. Native cannot establish whether a managed finally block actually executed.

The readiness budget remains 4096 attempts for this one cleanup root, including implicit missing-method transitions. Exhaustion rejects further work and phase.finish with `cleanup_budget_exhausted: native cleanup remains incomplete` while obligations remain. A host must explicitly close incomplete teardown and preserve measured CLR ownership. There are no automatic chunks or second recovery roots. Native status retains truthful outstanding counts after close.

Successful phase.finish requires exact claimed authority and drained native obligations. It flushes an existing deferred lifetime once, then enters Closed. It neither invents a lifetime when inactive nor advances time. Pending destruction remains available during cleanup callbacks and is retired only at successful finish. Duplicate finish and all later gameplay/recovery admission reject.

## Required runtime and Cdecl consumer

1. Negotiate this capability and the selected scheduler's existing capabilities. Do not infer recovery support from ordinary Cleanup. Require matching managed consumer support before advertising managed recovery guarantees.
2. Detect infrastructure failure separately from ordinary application callback reports. Wait until the synchronous Cdecl invocation returns and managed scopes unwind on the owner thread. Enforce a Rust dispatch-depth guard and managed scope nonreentrancy. A native Claimed bit cannot reveal the live CLR stack.
3. Call begin_cleanup before abort or close. Keep the host key in Rust. Put only the returned private context in the Cleanup Cdecl envelope. Claim once before any managed callback and use the established Cleanup NativePhaseScope. A phase-only adapter drains its existing managed scheduler; an activated native-readiness adapter drains native tickets. Never run both schedulers over the same graph.
4. Preserve failed-load/partial-binding ownership until recovery. Publish ownership before fallible binding, permit Cleanup of a partially loaded session, and avoid disposing or detaching it under Initialize. No gameplay reentry is allowed.
5. Complete entered native tickets in managed finally blocks when execution completion is known, continue cleanup peers after application exceptions, and preserve execution_unknown for abandoned attempts. Never retry entered Dispose after uncertain completion.
6. Finish only after the selected scheduler has drained. Aggregate primary, callback, cleanup, admission, transport and close errors with actual measured scripts/coroutines/bindings. Clear loaded ownership only after measured drain. Retain cleanup-pending ownership and reject reload when roots remain, even after native world destruction.
7. Classify unsupported provider, already Closed, admission failure, transport uncertainty, budget exhaustion and incomplete cleanup separately. On lost private context, recover saved response bytes or close incomplete. Never reconstruct credentials from status, replay begin/claim execution, reopen Closed or fabricate a zero-dt frame.

The historical 525 retained CLR-root failure and the 14 old-provider baseline probes remain unresolved managed evidence. Actual hostfxr/Cdecl failure recovery, iterator finally execution, failed partial binding, recursive host rejection and measured zero managed roots require the next runtime/managed lane. Native world drain alone proves none of those outcomes.
