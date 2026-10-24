# G02 — Lifecycle, execution, coroutines, and time

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Desired Unity 6.3 compatibility is unverified. Every code/signature is an authored illustrative subset from memory, not a complete declaration or compile-tested example. Relevant uncertainties: U001–U003, U007–U009.

**Boundary:** engine `MonoBehaviour`, `Time`, yield instructions, `UnityEngine.LowLevel.PlayerLoop`, and related scheduling. Awaitable/task support is version-sensitive and is not interchangeable with coroutines.

## G02.T01 — Lifecycle callbacks

Common message shapes, declared on a MonoBehaviour rather than overrides of virtual base methods:

```csharp
void Awake();
void OnEnable();
void Start();                 // IEnumerator Start() is also a familiar coroutine form.
void FixedUpdate();
void Update();
void LateUpdate();
void OnDisable();
void OnDestroy();
void OnApplicationFocus(bool hasFocus);
void OnApplicationPause(bool paused);
void OnApplicationQuit();
```

`Awake` performs instance initialization; `OnEnable` responds to becoming enabled and active; `Start` runs once before the first frame-update participation. These are not constructors. Do not assume Awake has run merely because a reference was deserialized.

### Stage 4 baseline — lifecycle eligibility (J3-001)

For ordinary runtime instances, recalled eligibility is:

| Instance state / change | Expected callback eligibility |
|---|---|
| Active-in-hierarchy GameObject, disabled MonoBehaviour | Awake runs once for initialization even though this component is disabled; OnEnable and Start wait for enablement. |
| Initially inactive-in-hierarchy GameObject | Awake is deferred until the GameObject first becomes active in the hierarchy; setting activeSelf under an inactive parent is insufficient. |
| Enabled MonoBehaviour on an active-in-hierarchy GameObject | OnEnable runs on becoming active-and-enabled; Start runs once when enabled and eligible for its first update. |
| Disable then re-enable after Start | OnDisable then OnEnable; no second Awake or Start. |
| Destroy an object that has never been active | OnDestroy is not guaranteed; recalled ordinary delivery is limited to GameObjects that have previously been active. |

Runtime creation/activation can nest callbacks inside AddComponent, Instantiate, or SetActive calls. Do not postpone every such callback to a later frame or impose global ordering on peer objects. These are memory-based baseline rules, not evidence about every reload/teardown path (U007).

Proposed traces (not executed): active object plus initially disabled component logs Awake, then OnEnable and Start only after enablement; inactive instance logs no Awake until activation, then only eligible enable/start callbacks; destruction of a never-active instance must not be used as a guaranteed cleanup callback. Construct fixtures with the intended initial state rather than deactivating only after an already-active creation has initialized them.

`Update` and `LateUpdate` are rendered-frame-oriented; `FixedUpdate` is simulation-step-oriented. Exceptional teardown/reload paths, quit ordering, and unspecified peer ordering remain U007.

`Reset` is primarily an editor default-setup callback. `OnValidate` is editor validation, can occur outside ordinary gameplay timing, and should not be treated as an unrestricted main-thread construction hook. Rendering and physics message families belong to their subsystems. Exceptions in callbacks should be observable through logging and must not accidentally abort all unrelated scheduled objects in a replacement runtime.

## G02.T02 — Player loop and execution ordering

A conceptual frame contains input/event processing, zero or more fixed steps, dynamic updates, late updates, rendering, and end-of-frame work. This is not a full authoritative PlayerLoop listing. A slow frame can require multiple physics steps; a fast frame can require none. The same frame may observe different body state before and after interpolation/render preparation.

Order across unrelated scripts is not intrinsically deterministic. `[DefaultExecutionOrder(int)]` and editor execution-order settings provide ordering controls, but do not create a general dependency graph or total ordering among all callbacks. Scene-load initialization has stronger broad relationships than arbitrary runtime instantiation inside a callback. Avoid asserting “every Awake always precedes every OnEnable” for all creation scenarios.

`UnityEngine.LowLevel.PlayerLoop.GetCurrentPlayerLoop()`, `GetDefaultPlayerLoop()`, and `SetPlayerLoop(PlayerLoopSystem)` expose a low-level loop representation. `PlayerLoopSystem` contains type/subsystem/delegate-related fields. Exact field layout and subsystem inventory are not reproduced. Replacing the loop can remove essential engine work; it is not just registering one more Update listener.

## G02.T03 — Coroutines and yield instructions

```csharp
Coroutine StartCoroutine(System.Collections.IEnumerator routine);
Coroutine StartCoroutine(string methodName);
Coroutine StartCoroutine(string methodName, object value);
void StopCoroutine(Coroutine routine);
void StopCoroutine(System.Collections.IEnumerator routine);
void StopCoroutine(string methodName);
void StopAllCoroutines();
```

Starting a coroutine generally advances its enumerator immediately until its first yield. It is cooperative scheduling on the main thread, not parallel execution. Retain the relevant IEnumerator or Coroutine handle for stopping; creating a fresh iterator instance does not identify an already running one. Do not assume string-started and enumerator-started forms have identical stopping identity rules.

Remembered yield forms:

- `yield return null`: continue at a subsequent frame continuation point.
- `new WaitForSeconds(float)`: scaled-time wait; measured scheduling does not promise exact wall-clock duration. Waiting begins relative to an engine frame boundary, and resumes on an eligible later frame.
- `new WaitForSecondsRealtime(float)`: unscaled time. Its public mutable wait behavior and reset details should be checked before sharing instances.
- `new WaitForFixedUpdate()`: resume at a fixed-update-related phase, not “sleep for one fixedDeltaTime.”
- `new WaitForEndOfFrame()`: after rendering work at the end of a frame; editor focus/headless behavior can differ.
- `new WaitUntil(Func<bool>)`, `new WaitWhile(Func<bool>)`: predicate-based waits, commonly evaluated after Update and before LateUpdate. Newer timeout overloads are not enumerated.
- `yield return otherEnumerator` or a Coroutine: nested/composed execution; parent completion waits in the customary use.
- `yield return AsyncOperation`: wait for completion of a participating engine operation.

`CustomYieldInstruction` exposes `bool keepWaiting`; `YieldInstruction` is a base for built-ins. Destroying the MonoBehaviour or disabling its GameObject stops its coroutines in remembered behavior; merely setting `enabled = false` does not automatically stop them. Lifetime/cancellation of nested iterators and iterator disposal on every stop path remain U008. Coroutines do not automatically cancel independent requests they started.

## G02.T04 — Time and fixed stepping

```csharp
// Selected static Time properties.
float time { get; }
double timeAsDouble { get; }
float deltaTime { get; }
float unscaledTime { get; }
float unscaledDeltaTime { get; }
float fixedTime { get; }
float fixedDeltaTime { get; set; }
float timeScale { get; set; }
float maximumDeltaTime { get; set; }
float realtimeSinceStartup { get; }
int frameCount { get; }
float smoothDeltaTime { get; }
```

Double-precision variants exist for several clocks; this is not their complete inventory. `deltaTime` is context-sensitive: in FixedUpdate it corresponds to the fixed-step interval in normal usage. Scaled and unscaled clocks differ. `timeScale = 0` normally pauses scaled progression and ordinary fixed-step simulation, while Update can continue and unscaled waits can progress. TimeScale does not itself make a variable frame duration constant, pause DSP audio time, or prevent OS callbacks.

A useful scheduler maintains real elapsed time, scaled accumulated time, a fixed-step accumulator, and frame indices separately. Maximum delta constraints prevent unlimited catch-up; do not infer a precise dropped-time algorithm from this summary. Changing `fixedDeltaTime` when changing timeScale is application policy, not an automatic promise of the Time API. `Time.captureFramerate`/capture timing support offline capture but are outside the core subset.

Tests should control the scheduler's time source rather than sleep. Floating-point clocks lose resolution over long sessions; double clocks and frame-index-based assertions can avoid accidental precision failures.

## G02.T05 — Invoke, cancellation, asynchronous work

`Invoke(string methodName, float time)`, `InvokeRepeating(string methodName, float time, float repeatRate)`, `CancelInvoke()`, `CancelInvoke(string)`, and `IsInvoking(...)` provide name-based scheduled calls. They use engine timing rather than background timers. Invalid method signatures, zero/negative repeats, and scaled-time details need verification before being promised.

`AsyncOperation` commonly exposes `isDone`, `progress`, `allowSceneActivation`, `priority`, and `completed`. Not every derived operation uses every property meaningfully. Scene loads have special activation/progress behavior (G05).

### Stage 4 baseline — completed registration timing (J3-002)

In recalled engine AsyncOperation behavior, a handler registered in the frame the operation is created is invoked in a following frame even if the operation can finish synchronously. A handler registered after completion and after the completion event has already been invoked can run synchronously during registration. Initialize state needed by the handler before subscribing; do not rely on subscription always deferring work. This rule does not establish Awaitable continuation timing or Addressables Completed timing. Derived-operation/version details remain U009.

Proposed observations (not executed): record creation-frame subscription and callback frame for a fast operation; separately wait until an operation's completion event has been delivered, then subscribe another handler and observe whether it runs before the subscription statement returns. Merely polling isDone without establishing event delivery does not distinguish the latter case.

Unity 6-era `Awaitable` APIs are remembered, including `Awaitable.NextFrameAsync()`, `FixedUpdateAsync()`, `EndOfFrameAsync()`, `WaitForSecondsAsync(...)`, and main/background-thread transitions. A critical recalled ownership rule is that Awaitable instances are pooled: do not await the same instance more than once or treat it as a reusable/multi-consumer Task. Continuations run synchronously when completion is triggered, so completion can re-enter awaiting code; the completion thread matters, and a background-thread transition does not authorize engine-object access. Exact overloads, individual operations' completion phases, and cancellation/continuation edge cases remain U009, not a certified surface. `MonoBehaviour.destroyCancellationToken` and `Application.exitCancellationToken` are relevant remembered cancellation conveniences. Cache/access timing and editor reload behavior require checking. A `System.Threading.Tasks.Task` is a .NET abstraction, not inherently engine-lifetime-bound. Awaiting a task does not make engine calls thread-safe.

## G02.T06 — Minimal scheduler behavioral contracts

For a compatible subset, define observable states: created, initialized, active-and-enabled, started, disabled, pending-destruction, destroyed. Avoid exposing those internal labels as new Unity APIs. Test through callback logs, public active/enabled flags, and resulting state.

Suggested tests (not executed): enabling twice without an intervening disable does not duplicate Start; reactivation repeats OnEnable but not Start; zero fixed steps and multiple fixed steps are possible between rendered updates; scaled waits pause while realtime waits advance at timeScale zero; deactivating a GameObject stops its owned coroutine; component disable alone does not; callbacks caused by runtime object creation occur at the chosen verified point rather than being indiscriminately queued to next frame. Keep unspecified peer ordering out of assertions.

**Omitted:** a complete native PlayerLoop tree, all render callbacks, async GPU scheduling, editor domain-reload combinations, and a promise of exact coroutine phase ordering across all Unity 6 versions.
