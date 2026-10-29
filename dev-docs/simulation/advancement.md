# Native frame advancement

`world-simulation::World` owns one exact rational timeline and one callback scheduler. `TimeSpan::new` accepts nonnegative reduced seconds; `from_nanos` is the OS duration entry point. There are no execution-mode selectors or JSON operations. Arithmetic uses checked, reduced fractions with bounded numerator and denominator. Fixed-step admission rejects excess work before publishing the frame. Time is neither dropped nor clamped.

## Dispatch contract

Call `advance(elapsed)` once per frame. An `Advance::Callback` transfers an owned script handle, typed message and ticket to the host. Release native locks and borrows, invoke script code, then pass its outcome to `resume`. Only the top callback ticket may resume. A wrong, foreign, reused or suspended parent ticket is rejected without consuming the current callback. `Advance::Complete` ends the frame. Starting another frame while a callback is outstanding fails.

The order is:

1. Pending Awake and activation transitions from native lifecycle readiness (`lifecycle.rs`).
2. Start for eligible scripts that have not started.
3. Each admitted fixed step: fixed input projection, Start if newly eligible, FixedUpdate, 2D physics, contact callbacks, fixed coroutine resumptions.
4. Dynamic input projection, Start if newly eligible, Update.
5. Due next-frame, scaled and unscaled coroutine resumptions.
6. The delayed-tasks callback, when `set_delayed_tasks` names a host identity: one asset callback with `Update` for the host's main-thread work queue. Its identity cannot be a script asset's or zero.
7. Start if newly eligible, LateUpdate.
8. Disable/destroy cleanup and native storage retirement.

Stable component-handle order determines each ordinary script phase. Contact order is the physics pair/phase order followed by recipient handle order. Contacts reach scripts on collider objects and body owners, with duplicate recipients removed. `Contact2d` carries a native contact ID, resolved with `contact_event`; contact eligibility is checked again before every delivery. A callback mutation can suppress a later contact recipient.

Each phase snapshots its candidates when it starts. Between callbacks the scheduler drains new lifecycle transitions and discovers new scripts. It rechecks live eligibility before ordinary work. Start is recorded only when actually dispatched. A script introduced during Update receives immediate lifecycle work, then Start before LateUpdate; it does not receive an Update from a phase whose candidate list already exists. Disabling a script suppresses its remaining ordinary callbacks. Destroying a subtree suppresses ordinary work immediately and defers storage retirement until cleanup.

The lifecycle `awake` and `active` flags represent native lifecycle readiness and scheduled transitions. The scheduler separately records whether Awake or OnEnable has entered, using Dormant, Awake and Enabled states. An obsolete pending OnEnable cannot enter twice without OnDisable. A peer deactivated before its pending Awake waits for reactivation. If a new activation batch reaches that peer first, Awake enters before OnEnable and the older pending Awake is coalesced. Queued disable transitions retain their batch order, including a disable queued before the initial enable entered. The scheduler consumes lifecycle mutation batches rather than deriving a second activation policy. The low-level batch APIs (`lifecycle_work`, `take_lifecycle_work`) are available to native consumers; a host driving `advance/resume` must not also consume those batches or retire storage independently.

## Immediate clone lifecycle

`instantiate` completes two-pass allocation and reference remapping before publishing a mutation ID. Call `next_lifecycle(mutation)` to open a child dispatch scope. Drain its Awake/OnEnable work through `resume` until `Advance::LifecycleComplete`, then return the clone to the caller. The parent's ticket remains suspended until that child scope closes. Nested clones use the same stack. Start remains owned by the next eligible ordinary phase.

The same operation supports an immediate lifecycle scope outside a frame without advancing time. A failure in a child cancels all suspended parent scopes. The host must propagate that failure rather than resume the parent.

## Time and input

`Clock::time` and `unscaled_time` are admitted frame endpoints. `fixed_time` is completed fixed-phase time. `delta` and `unscaled_delta` expose the current fixed or dynamic phase's deltas. Dynamic deltas are restored before Update. Admission preflights every intermediate fixed-time sum, fixed-frame number, input cutoff and fixed unscaled delta. Execution publishes those validated phase values without repeating fallible time arithmetic. The fixed accumulator carries exact fractional remainder between frames. The default fixed step is 1/50 second, with a maximum of 1024 steps; configure the fixed step and budget before the first frame. Time scale is in 0..=100. A time-scale write during a callback affects the next frame admission; it does not change already admitted fixed steps, deltas or input cutoffs. A scale of zero pauses fixed steps, scaled waits and delayed scaled destruction, while unscaled time, input, Update and unscaled waits continue.

Input events have nondecreasing unscaled timestamps. Equal timestamps retain submission order. A batch is validated before any event is accepted. Events at or before a published cutoff cannot be inserted later. Both projections include events exactly at their cutoff. Fixed cutoffs interpolate the current frame's unscaled interval from the old scaled accumulator and current time scale. Dynamic input uses the frame endpoint. Fixed and dynamic projections consume edges independently; a second fixed step does not repeat the first step's edge. Events remain queued until both cursors consume them, subject to a 4096-event queue limit.

Focus loss releases every held key and clears device axes. Input while unfocused does not restore held state. Typed keyboard-axis bindings project negative and positive keys to raw -1/0/1 values, optionally inverted. Explicit normalized axis events override keyboard bindings for their axis.

Named axes (`World::register_input_axis`, `crates/world-simulation/src/input_axes.rs`) layer Unity Input Manager semantics on those bindings: a name, gravity, sensitivity, dead zone and snap. `input_axis(phase, name, raw)` returns the raw value with dead zone and invert applied, or the smoothed value, which is tracked separately for the fixed and dynamic phases and reset by focus loss. Several IDs may share a name; the reading with the greatest magnitude wins, and the lowest ID breaks ties. No axes are registered implicitly, and an unknown name is an error.

## Coroutines, failure and host integration

Managed iterator bodies remain managed. Native coroutine IDs own target eligibility, deadlines, phase selection and cancellation. `start_coroutine` registers a wait after the host has executed the initial iterator body. `Coroutine(id)` asks the host to run the next body. `Yielded(wait)` schedules the next resumption; `Returned` completes it. Dynamic waits cannot resume again in the same frame. Fixed waits resume after physics in an eligible fixed phase. End-of-frame waits (wait kind 4) resume once, after every `LateUpdate` of the frame and before cleanup. A coroutine that yields one while resuming from that phase waits for the end of the next frame. A coroutine started before a frame's end phase resumes at the end of that frame. Component disable preserves coroutine waits and resumptions while suppressing ordinary script phases. Object deactivation and target destruction cancel coroutines immediately, including destruction after the coroutine phase.

`take_cancelled_coroutines()` transfers cancelled IDs to the host and consumes those notifications. Deactivation, ancestor deactivation, destruction, explicit stop and callback failure publish each removed ID once. Notifications remain native-owned until drained, even after target storage retires. Cancellation operations retain call order; each target's IDs and failure-wide remaining IDs use ID order. Cancellation is published immediately, before the associated disable/destroy callbacks enter. The host drains notifications after mutations and advance/resume, disposes each iterator once, and defers disposal of an iterator currently on the callback stack until its body exits. A cancelled callback must still return its ticket; a yielded wait cannot resurrect it and its deadline is not evaluated. Normal `Returned` completion removes the ID without a cancellation notification because the host already owns that completion.

`Failed(ScriptFailure)` cancels ordinary callbacks and coroutines, invalidates all live objects, and dispatches disable/destroy work before retiring storage. Unentered teardown tasks survive abandonment of ordinary scopes, including when the first failure occurs during cleanup. Already entered or completed cleanup is not repeated. New allocation is rejected during failed-advance teardown, so cleanup cannot replenish the world indefinitely. Cleanup failures do not replace the first failure or restart teardown. After cleanup, the original failure is returned and later advancement fails with `Faulted`. Exceptions never cross the C ABI.

The C ABI exposes this contract as `disunity_world_advance` and `disunity_world_resume` in `crates/disunity-ffi/src/scheduling.rs`, and `DisunityHost` drives managed scripts through them. There is no separate ABI physics step. Native callers without scripts can still call `step_physics` directly, but a direct physics step during an active callback scope is rejected to prevent double stepping. Scheduled frames invoke the same physics implementation internally.
