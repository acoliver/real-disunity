# G11 — uGUI, EventSystems, TextMesh Pro, UI Toolkit

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Desired Unity 6.3 and UI package versions are unverified. All code/signatures are authored illustrative subsets from memory, not full declarations or compile-tested examples. See U001–U003 and U034–U036.

**Do not merge these stacks:** Canvas-based uGUI (`UnityEngine.UI`), event routing (`UnityEngine.EventSystems`), TMP (`TMPro`), and retained-mode UI Toolkit (`UnityEngine.UIElements`) have different component/tree models. Canvas/RectTransform/CanvasRenderer support is engine-side; uGUI is distributed as a package, commonly `com.unity.ugui`. TMP historically has `com.unity.textmeshpro`, with distribution/integration changing over time. UI Toolkit has engine/editor modules and version-dependent package history. Exact Unity 6.3 packaging is U034, not asserted from namespace names.

## G11.T01 — Canvas and uGUI layout

`Canvas` defines `RenderMode.ScreenSpaceOverlay`, `ScreenSpaceCamera`, or `WorldSpace`. Properties include renderMode, worldCamera, planeDistance, sortingLayerID/sortingOrder, overrideSorting, pixelPerfect, scaleFactor, referencePixelsPerUnit and rootCanvas. CanvasRenderer submits UI geometry; it is not MeshRenderer.

`CanvasScaler` offers remembered modes `ConstantPixelSize`, `ScaleWithScreenSize`, `ConstantPhysicalSize` through `CanvasScaler.ScaleMode`; referenceResolution and screenMatchMode/matchWidthOrHeight influence resolution scaling. Actual physical DPI can be unreliable. Overlay UI can use a null event camera, whereas world-space/camera UI needs a meaningful camera context.

uGUI geometry is built on RectTransform (G04). Layout interfaces/components include `ILayoutElement`, `ILayoutController`, `LayoutElement`, `HorizontalLayoutGroup`, `VerticalLayoutGroup`, `GridLayoutGroup`, `ContentSizeFitter`, `AspectRatioFitter`, and `LayoutRebuilder`. Minimum, preferred, and flexible sizes are distinct inputs. Multiple components driving the same dimensions can create cycles or overrides. `Canvas.ForceUpdateCanvases()` and `LayoutRebuilder.ForceRebuildLayoutImmediate(RectTransform)` are explicit synchronization tools; normal layout is deferred/batched.

`CanvasGroup` controls alpha, interactable, blocksRaycasts, and ignoreParentGroups. Alpha zero does not by itself mean an object is non-interactive. Deactivating the GameObject is different from disabling a Graphic, setting color alpha, or blocking raycasts.

## G11.T02 — uGUI controls and events

`Graphic` provides color, material, raycastTarget and dirty/rebuild concepts. `Image` adds sprite, overrideSprite, type (`Simple`, `Sliced`, `Tiled`, `Filled`), preserveAspect and fill settings. `RawImage` displays a Texture and UV rect. Legacy `Text` provides string/font/fontSize/alignment; it is not TMP_Text.

`Selectable` is the shared interaction base for `Button`, `Toggle`, `Slider`, `Scrollbar`, `Dropdown`, and `InputField`. Familiar public shapes:

```csharp
// Selected property/event usage forms.
button.onClick.AddListener(HandleClick);
toggle.isOn = true;
toggle.onValueChanged.AddListener(HandleToggle);
slider.value = 0.5f;
slider.SetValueWithoutNotify(0.25f);
inputField.text = "draft";
inputField.onEndEdit.AddListener(HandleEdited);
```

These are usage fragments, not standalone compilable programs. Value-without-notify families exist for several controls but should be checked per type rather than inferred everywhere. Programmatic value assignment can invoke a value-change event; it is not always equivalent to direct field mutation. Button.onClick is a completed click semantic, not simply pointer-down. Interactable, navigation, transition colors/sprites/animation, and current selection affect control behavior.

`ScrollRect` coordinates viewport/content, normalized position, velocity, inertia, elasticity and scrollbars. `Mask` and `RectMask2D` have different masking mechanisms. Mask clipping, raycast filtering, sibling ordering and nested canvases should be tested rather than inferred from renderer visibility. TMP controls (`TMP_InputField`, `TMP_Dropdown`) are distinct classes with related but not identical APIs.

## G11.T03 — EventSystem and raycasting

Key types: `EventSystem`, `BaseInputModule`, `PointerInputModule`, `StandaloneInputModule`, `BaseRaycaster`, `GraphicRaycaster`, `PhysicsRaycaster`, `Physics2DRaycaster`, `BaseEventData`, `PointerEventData`, `AxisEventData`, `RaycastResult`, and `ExecuteEvents`.

Common receiver interfaces:

- `IPointerEnterHandler`, `IPointerExitHandler`, `IPointerDownHandler`, `IPointerUpHandler`, `IPointerClickHandler`.
- `IBeginDragHandler`, `IDragHandler`, `IEndDragHandler`, `IInitializePotentialDragHandler`, `IDropHandler`, `IScrollHandler`.
- `ISelectHandler`, `IDeselectHandler`, `IMoveHandler`, `ISubmitHandler`, `ICancelHandler`.

Methods follow shapes such as `void OnPointerClick(PointerEventData eventData)`. PointerEventData includes position/delta, button, pointerId, pressPosition, pointerPress/drag objects, and current/press raycast information. Selection and hover are not identical. A drag can cancel or alter click behavior depending on press/drag thresholds and receivers.

`EventSystem.current`, `currentSelectedGameObject`, `SetSelectedGameObject(GameObject)`, `RaycastAll(PointerEventData, List<RaycastResult>)`, and `IsPointerOverGameObject()`/ID overload are remembered. Pointer IDs differ between mouse, touch and Input System modules; do not hard-code one ID interpretation across modules. Raycast hit ordering combines raycaster/canvas/sorting/distance rules. No universal “closest world object wins” rule suffices for overlay UI.

Input System's `InputSystemUIInputModule` belongs to the Input System package (G08). Multiple active input modules/EventSystems require deliberate ownership; they are not automatically harmless duplicates. UI event dispatch is not physics collision callback dispatch.

## G11.T04 — TextMesh Pro

`TMP_Text` is a common base; `TextMeshProUGUI` renders in Canvas UI and `TextMeshPro` renders scene text through a different renderer path. Important properties include text, font (`TMP_FontAsset`), fontSize, color, alignment, enableAutoSizing, fontSizeMin/fontSizeMax, richText, overflowMode, maxVisibleCharacters, and textInfo.

```csharp
// Selected TMP_Text shapes.
void SetText(string sourceText, bool syncTextInputBox = true);
void ForceMeshUpdate(bool ignoreActiveState, bool forceTextReparsing);
Vector2 GetPreferredValues(string text);
```

The recalled string SetText declaration includes an optional syncTextInputBox parameter; a one-argument call does not establish a separate one-parameter method for reflection or delegate binding. Formatted/numeric SetText overloads and optional ForceMeshUpdate arguments also exist; exact declarations in the selected package remain U035. A text setter can defer mesh/layout generation. `textInfo` contains character/word/line/link/mesh-related results only after appropriate generation. Code-point count, UTF-16 string length, glyph count and visible character count are not interchangeable.

`TMP_FontAsset` represents font/glyph/atlas metadata, with fallback chains and static/dynamic atlas options. A font file is not itself a ready TMP asset. Rich text parsing, fallback glyph selection, wrapping, line breaking, kerning/shaping, bidirectionality, sprite assets and material references are substantial subsystems; do not claim that drawing characters at equal spacing is TMP-compatible. Imported atlas data and exact package schemas are outside this reference.

## G11.T05 — UI Toolkit retained trees

UI Toolkit constructs `VisualElement` trees rather than one GameObject per widget. `UIDocument` attaches a visual tree asset and panel settings to a runtime GameObject. `VisualTreeAsset` represents UXML-authored structure, `StyleSheet` represents USS, and `PanelSettings` configures panel rendering/scaling/input context. `rootVisualElement` provides the live tree entry point.

```csharp
// Selected VisualElement/asset usage shapes.
var root = document.rootVisualElement;
var confirm = root.Q<UnityEngine.UIElements.Button>("confirm");
confirm.clicked += HandleConfirm;
root.Add(new UnityEngine.UIElements.Label("Ready"));
visualTreeAsset.CloneTree(root);
```

This illustrative fragment assumes variables/handlers and is not compile-tested. `Q<T>`/`Query<T>` are tree query conveniences with overloads for names/classes. `name`, `viewDataKey`, CSS-like class lists, hierarchy operations, `userData`, `style`, `resolvedStyle`, `layout`, `worldBound`, `visible`, `enabledSelf`, `enabledInHierarchy`, `pickingMode`, and `focusable` are common concepts. VisualElement is not a UnityEngine.Object/Component; do not apply Unity-null or GetComponent to it.

Controls include Label, Button, Toggle, TextField, IntegerField, FloatField, Slider, ScrollView, ListView, TreeView, DropdownField, and editor-only fields in `UnityEditor.UIElements`. Virtualized ListView items must be bound/rebound as records change; element instance identity is not record identity. Exact list APIs and newer controls are U036.

## G11.T06 — UI Toolkit styling, events, binding boundaries

USS styling is CSS-like, not web CSS compliance. Flex layout, display, visibility, position, dimensions, margins/padding, colors and transforms are exposed through style wrappers; resolvedStyle reports computed values, which may only be valid after a layout pass. `display: none`, hidden visibility, disabled controls and `PickingMode.Ignore` affect different behaviors.

Typed event registration follows `RegisterCallback<TEventType>(...)` and matching unregister methods. `TrickleDown.TrickleDown` opts into the downward phase where supported; default propagation includes target/bubbling semantics. Pointer events, ClickEvent, FocusIn/Out, KeyDown/Up, GeometryChangedEvent and ChangeEvent<T> are distinct families. `StopPropagation`, `StopImmediatePropagation`, and default-action prevention have related but not interchangeable meanings, with version-sensitive APIs. Pointer capture routes later pointer events independently of current hover; capture must be released on appropriate lifetime paths.

Value controls commonly use `value`, `RegisterValueChangedCallback(...)`, and `SetValueWithoutNotify(...)`. Runtime binding, property bags/data sources and editor `SerializedObject` binding are different systems; exact Unity 6 binding interfaces are U036 and intentionally not invented. UXML factories/traits versus newer source-generated/attribute-driven custom controls are also version-sensitive. UI Toolkit and uGUI can coexist, but focus, event priority and sorting are not automatically unified.

Proposed tests (not executed): disabled control does not emit a user click; alpha-zero versus raycast-blocking distinction; layout settles before geometry observation; pointer press/drag/up produces the intended click outcome; value-without-notify suppresses only the change event; event propagation and pointer capture; and recycled ListView entries display the correct records. Exact typography, UXML/USS schemas, runtime data binding and package integration remain incomplete.
