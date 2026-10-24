# G08 — Legacy Input and package Input System

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6.3 and package versions are unverified. All code/signatures are authored illustrative subsets from memory, not full declarations or compile-tested examples. See U001–U003, U025–U027.

**Critical distinction:** `UnityEngine.Input` is the legacy engine Input Manager surface. `UnityEngine.InputSystem` is a separately versioned package, commonly `com.unity.inputsystem`. They do not share one polling buffer or configuration model merely because both read hardware. Player active-input settings can enable one or both; availability and error behavior must be explicit.

## G08.T01 — Legacy frame polling

```csharp
// Selected static UnityEngine.Input members.
static bool GetKey(KeyCode key);
static bool GetKeyDown(KeyCode key);
static bool GetKeyUp(KeyCode key);
static bool GetMouseButton(int button);
static bool GetMouseButtonDown(int button);
static bool GetMouseButtonUp(int button);
static bool GetButton(string buttonName);
static bool GetButtonDown(string buttonName);
static bool GetButtonUp(string buttonName);
static float GetAxis(string axisName);
static float GetAxisRaw(string axisName);
static Vector3 mousePosition { get; }
static Vector2 mouseScrollDelta { get; }
static string inputString { get; }
```

String key-name forms also exist historically. KeyCode names commonly include `Space`, `Return`, `Escape`, arrow keys, letter keys, modifier keys, and joystick entries. An exhaustive numeric enum mapping is not supplied. Mouse button indexes conventionally 0 left, 1 right, 2 middle.

GetKey reports held state; Down/Up report transitions sampled for a frame, not a destructive dequeue per call. Repeated calls in the same frame should see the same transition. They should ordinarily be read in Update, not only in FixedUpdate, because a rendered frame can have zero or multiple fixed ticks. “Pressed this frame” and “currently held” are separate observations. Button names map to configured logical inputs; they are not necessarily key names.

Mouse position is screen-pixel space with the legacy convention of bottom-left origin; out-of-window positions can occur. Input text is not a complete internationalized text-editing protocol. KeyCode polling, text entry, and IME composition are separate requirements.

## G08.T02 — Legacy axes, touches, sensors

Legacy named axes come from Input Manager configuration, including sensitivity, gravity, dead zones, inversion, and device selection. GetAxis can smooth digital input; GetAxisRaw commonly bypasses smoothing. Mouse axes can represent deltas and should not be assumed clamped to -1..1. A fallback that always returns keyboard -1/0/1 is not generally axis-compatible.

`Input.touchCount`, `GetTouch(int)`, and `touches` expose `Touch` values. Remembered fields/properties include fingerId, position, rawPosition, deltaPosition, deltaTime, tapCount, phase, pressure, maximumPossiblePressure, radius, and type. Availability and validity vary by device. `TouchPhase.Began`, `Moved`, `Stationary`, `Ended`, `Canceled` are familiar. Array index is not persistent finger identity. `simulateMouseWithTouches` and multi-touch settings affect compatibility behavior.

Sensor-related legacy APIs include `acceleration`, acceleration event samples, `gyro`, `compass`, and `location`. Location service start/stop/status requires permissions and can fail or remain initializing. Accelerometer/gyro coordinates and device orientation mapping need platform testing. `Input.compositionString`, IME composition mode and cursor position support text composition; `TouchScreenKeyboard` is a separate engine-facing mobile keyboard abstraction.

`Cursor.visible`, `Cursor.lockState`, and `CursorLockMode.None`, `Locked`, `Confined` are adjacent engine APIs rather than Input Manager bindings. Focus loss and OS security policies can override pointer capture. Never assume mouse lock is a pure persistent field.

## G08.T03 — Package devices and controls

The Input System models `InputDevice`, `InputControl`, typed controls, layouts, state events, usages, and processors. Remembered access patterns:

```csharp
// Illustrative package usage shapes, not a full declaration.
Keyboard.current.spaceKey.isPressed;
Keyboard.current.spaceKey.wasPressedThisFrame;
Mouse.current.position.ReadValue();
Mouse.current.delta.ReadValue();
Gamepad.current.leftStick.ReadValue();
Gamepad.current.buttonSouth.wasReleasedThisFrame;
```

`Keyboard.current`, `Mouse.current`, and `Gamepad.current` can be null and can change as devices are used/added/removed. “Current” is not necessarily the device assigned to a particular player. Keyboard physical control identity is not equivalent to typed characters or display layout. A stick produces Vector2; buttons have press thresholds rather than necessarily boolean raw storage.

`InputSystem.devices`, `onDeviceChange`, `AddDevice`, `RemoveDevice`, `QueueStateEvent`, and `Update` are remembered package APIs. Synthetic device/event APIs are especially useful for tests, but manual Update can interfere with automatic update mode. Exact generic shapes, timestamps, supported layouts, and event lifetimes are U026.

Sensors and touch are package devices too. `Touchscreen` low-level touch controls and the `UnityEngine.InputSystem.EnhancedTouch` API are distinct views; enhanced touch requires explicit support enablement in remembered usage. Do not claim that the legacy `Input.touches` array is the package's authoritative source.

## G08.T04 — Actions and interactions

Key types: `InputAction`, `InputActionMap`, `InputActionAsset`, `InputBinding`, `InputActionReference`, `InputActionProperty`. An action maps controls to semantic intent, optionally through processors, composite bindings and interactions.

```csharp
// Selected InputAction shapes.
void Enable();
void Disable();
TValue ReadValue<TValue>() where TValue : struct;
bool IsPressed();
bool WasPressedThisFrame();
bool WasReleasedThisFrame();
```

Common construction/binding use includes `new InputAction(name, InputActionType.Button)`, `AddBinding("<Keyboard>/space")`, and composite setup via `AddCompositeBinding("2DVector").With(...)`. The fluent API's exact overloads and optional arguments are not certified. `InputActionType.Button`, `Value`, and `PassThrough` differ in change/actuation and conflict-resolution behavior.

`started`, `performed`, and `canceled` callbacks take `InputAction.CallbackContext`. The context exposes phase/action/control/time and `ReadValue<TValue>()` in familiar forms. It is a transient callback view; copy needed values rather than persisting it as an independent input snapshot. `InputActionPhase.Disabled`, `Waiting`, `Started`, `Performed`, `Canceled` are recalled. Interactions such as press, tap, hold, slow tap, multi-tap alter the phase sequence; a performed event is not synonymous with every raw button-down. Disabling an in-progress action can cause cancellation.

Processors can apply dead zones, inversion, normalization and scaling. Composite bindings combine controls. Store binding overrides separately from imported asset definitions where appropriate. Exact interactions' timing and thresholds require a selected package version (U027).

## G08.T05 — PlayerInput and UI integration

`PlayerInput` associates actions, devices/control schemes, and notification behavior with a player. Remembered members include `actions`, `currentActionMap`, `SwitchCurrentActionMap(string)`, `ActivateInput()`, `DeactivateInput()`, and camera/UI-related configuration. Notification modes include Send Messages, Broadcast Messages, Unity Events, and C# Events through `PlayerNotifications` in remembered versions. Do not invent identical callback argument types for all modes.

`PlayerInputManager` manages joining and local multiplayer. Device pairing/user state uses Input System user APIs; selecting Gamepad.current for every player defeats pairing. `InputSystemUIInputModule` integrates package actions with `EventSystem`; legacy `StandaloneInputModule` uses the older input stack. Running both can duplicate UI events or create confusing focus behavior.

UI focus does not automatically suppress every gameplay action. Action-map switching, UI mode, and player ownership are application policy. Virtual controls and on-screen input are package facilities, not evidence that legacy axis configuration is loaded.

## G08.T06 — Update timing, rebinding, compatibility

Input System update mode can process events in dynamic updates, fixed updates, or manually, depending on package settings. “This frame” is relative to the package's update semantics; physics simulation and rendering can observe different snapshots. OS events can arrive with timestamps and multiple state changes in one engine frame. A compatible input layer must choose how to preserve taps and event ordering rather than sampling hardware once and discarding intermediate changes.

Interactive rebinding is remembered through `PerformInteractiveRebinding()` and an operation with completion/cancel callbacks and disposal. Binding override JSON save/load helpers exist; exact names/options and conflict handling are U027. Persistence is distinct from OS device layout. Rebinding operations require cleanup and should not leave unintended actions disabled indefinitely.

Proposed tests (not run): frame-consistent Down polling; a short tap between fixed ticks is not lost; no device returns a safe null current-device observation; one player's paired device does not drive another; action disable produces the chosen cancellation semantics; hold/tap behavior uses controlled event time; UI receives one click through the selected module. This group does not specify HID layouts, platform backends, every interaction, complete IME behavior, or exact package serialization.
