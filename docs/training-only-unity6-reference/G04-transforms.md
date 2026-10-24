# G04 — Transforms and spatial hierarchy

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6 concepts, not verified Unity 6.3 behavior. Code/signatures are authored illustrative subsets from memory, neither complete declarations nor compile-tested examples. Uncertainties: U001–U003, U013–U015.

**Boundary:** engine `UnityEngine.Transform` and `RectTransform`; layout drivers and physics bodies impose additional rules. Every normal GameObject has one Transform-family component.

## G04.T01 — Local and world TRS

```csharp
Vector3 position { get; set; }
Vector3 localPosition { get; set; }
Quaternion rotation { get; set; }
Quaternion localRotation { get; set; }
Vector3 eulerAngles { get; set; }
Vector3 localEulerAngles { get; set; }
Vector3 localScale { get; set; }
Vector3 lossyScale { get; }
Vector3 right { get; set; }
Vector3 up { get; set; }
Vector3 forward { get; set; }
Matrix4x4 localToWorldMatrix { get; }
Matrix4x4 worldToLocalMatrix { get; }
void SetPositionAndRotation(Vector3 position, Quaternion rotation);
void SetLocalPositionAndRotation(Vector3 localPosition, Quaternion localRotation);
```

Local position/rotation/scale are relative to the parent. At a root, local position and local rotation correspond to world values. A conceptual world matrix is `parent.localToWorldMatrix * TRS(localPosition, localRotation, localScale)`. Setting world position solves for local position through the inverse parent transform; setting world rotation accounts for parent rotation. Scale and shear make exact world decomposition more difficult.

`lossyScale` is read-only because a hierarchy with rotation and non-uniform scale can generate shear. It is not a universally exact diagonal extracted from the full matrix. `right`, `up`, `forward` refer to rotation-derived axes, not arbitrary scaled columns. Euler values are alternate rotation representations, not reliable accumulated turn counters. Repeated read-modify-write of Euler components can create discontinuities.

Zero scale, reflected/negative scale, singular matrices, and non-unit quaternion assignments deserve dedicated tests (U013). A replacement runtime should not hide those edge cases behind a generic matrix decomposition that changes ordinary hierarchy behavior.

## G04.T02 — Hierarchy and parenting

```csharp
Transform parent { get; set; }
Transform root { get; }
int childCount { get; }
Transform GetChild(int index);
Transform Find(string name);
void SetParent(Transform parent);
void SetParent(Transform parent, bool worldPositionStays);
bool IsChildOf(Transform parent);
int GetSiblingIndex();
void SetSiblingIndex(int index);
void SetAsFirstSibling();
void SetAsLastSibling();
void DetachChildren();
```

`SetParent(newParent, true)` attempts to preserve world-space placement by adjusting local values; `false` preserves the local relationship values. The commonly used shorter SetParent defaults to preserving world position. Assigning `parent` also has historical world-preserving behavior; exact handling of rotation/scale and RectTransform warnings is U013. A Transform cannot become its own ancestor. Parent changes alter activity-in-hierarchy and can change scene ownership in engine-specific ways.

`Transform.Find` searches immediate-child names or a slash-delimited relative path, not an arbitrary recursive descendant-by-name query. It can locate inactive children. Global `GameObject.Find` is different. Child enumeration and sibling order represent hierarchy order, not execution order. Mutating a hierarchy while iterating may invalidate assumptions about indexes.

Common callbacks include `OnTransformParentChanged()` and `OnTransformChildrenChanged()`. RectTransform has additional dimension-change callbacks. These are engine messages whose exact timing under bulk edits is not specified here.

## G04.T03 — Point, vector, direction conversion

```csharp
Vector3 TransformPoint(Vector3 position);
Vector3 InverseTransformPoint(Vector3 position);
Vector3 TransformVector(Vector3 vector);
Vector3 InverseTransformVector(Vector3 vector);
Vector3 TransformDirection(Vector3 direction);
Vector3 InverseTransformDirection(Vector3 direction);
```

Scalar-coordinate overloads are common for these operations; bulk span/array variants in newer versions are not enumerated.

- Point conversions include translation, rotation, and scale.
- Vector conversions include rotation and scale, but no translation.
- Direction conversions conventionally include rotation, not translation or scale, preserving direction length in the usual contract.

Normals under non-uniform scale are not general TransformDirection inputs: use an inverse-transpose normal transform when implementing geometry/shading. Camera ray directions and movement axes should be intentionally chosen rather than switching indiscriminately among these methods.

Useful observable checks: transforming local origin gives world position; converting a point to world and back returns the original within tolerance for nonsingular transforms; non-uniform scale changes TransformVector's length but not TransformDirection's; reparenting with worldPositionStays false retains local values.

## G04.T04 — Movement and rotation helpers

```csharp
void Translate(Vector3 translation, Space relativeTo);
void Translate(Vector3 translation, Transform relativeTo);
void Rotate(Vector3 eulers, Space relativeTo);
void Rotate(Vector3 axis, float angle, Space relativeTo);
void RotateAround(Vector3 point, Vector3 axis, float angle);
void LookAt(Transform target);
void LookAt(Vector3 worldPosition, Vector3 worldUp);
```

`Space.Self` and `Space.World` are the common enum values. Short overloads often use self space; exact defaults should be checked before signature generation. Translate applies a displacement, not a speed and not automatically deltaTime-scaled. Rotate uses degrees; axis-angle and Euler rotation overloads have different interpretations. RotateAround changes both position and orientation around a world-space pivot/axis. LookAt aims the Transform's +Z axis toward a target with an up hint; collinear/zero vectors are degenerate cases.

Directly changing a Transform is teleport-like from a physics perspective unless a particular integration layer interprets it otherwise. Use Rigidbody/ Rigidbody2D movement APIs for physics-driven movement where their contracts apply. Visual interpolation and collision resolution are not supplied by Transform.Rotate or Translate themselves.

## G04.T05 — RectTransform

`RectTransform : Transform` represents a rectangle relative to a parent's rectangle and anchor reference. Selected properties:

```csharp
Vector2 anchorMin { get; set; }
Vector2 anchorMax { get; set; }
Vector2 anchoredPosition { get; set; }
Vector3 anchoredPosition3D { get; set; }
Vector2 sizeDelta { get; set; }
Vector2 pivot { get; set; }
Vector2 offsetMin { get; set; }
Vector2 offsetMax { get; set; }
Rect rect { get; }
void GetLocalCorners(Vector3[] fourCornersArray);
void GetWorldCorners(Vector3[] fourCornersArray);
void SetSizeWithCurrentAnchors(RectTransform.Axis axis, float size);
void SetInsetAndSizeFromParentEdge(RectTransform.Edge edge, float inset, float size);
```

Anchors are normalized positions in the parent rectangle; they can stretch independently on each axis. Pivot is the rectangle's local reference point. `sizeDelta` is relative to the anchor span, not always the final width/height. `anchoredPosition` measures the pivot relative to the anchor reference, not simply localPosition under all anchor configurations. Offset values and anchored position/size are coupled representations.

`Axis.Horizontal`/`Vertical` and `Edge.Left`/`Right`/`Top`/`Bottom` are remembered enums. `RectTransformUtility` offers `ScreenPointToLocalPointInRectangle`, `ScreenPointToWorldPointInRectangle`, `RectangleContainsScreenPoint`, and pixel-adjustment helpers. The camera is often null for screen-space overlay, but not universally for all canvases. Exact overloads and driven-property interactions are U014.

Layout systems can overwrite RectTransform fields after scripts set them. Reading immediately before layout has rebuilt can return stale geometry. `ForceUpdateRectTransforms` and layout/canvas rebuild helpers are explicit synchronization tools, not a reason to force all layouts every frame.

### Size and offset relationships (J3-004)

For an ordinary RectTransform parent, without a layout driver overriding the values, the recalled componentwise relationships are:

```text
rect.size = parent.rect.size * (anchorMax - anchorMin) + sizeDelta
offsetMin = anchoredPosition - sizeDelta * pivot
offsetMax = anchoredPosition + sizeDelta * (Vector2.one - pivot)
```

These are equations, not assignable C# declarations; `*` means componentwise multiplication here. The parent size is its local rectangle, not renderer world bounds. Rotation, localScale, Canvas scaling and perspective can change the displayed world/screen dimensions without changing these rectangle-size relationships. The anchor reference interpolates between the two anchors according to the pivot, separately on each axis.

- Fixed anchors: parent size (800,600), anchorMin = anchorMax = (0.5,0.5), pivot (0.5,0.5), sizeDelta (200,100), anchoredPosition (10,20). Rectangle size is (200,100); offsetMin is (-90,-30), offsetMax (110,70).
- Fully stretched anchors: the same parent, anchorMin (0,0), anchorMax (1,1), pivot (0.5,0.5), sizeDelta (-40,-20), anchoredPosition (0,0). Rectangle size is (760,580); offsetMin (20,10), offsetMax (-20,-10), representing inward margins.

A non-RectTransform parent and driven/deferred layouts need a separately selected contract (U014). Proposed observations (not run): both numeric examples after layout settles; change parent size and compare fixed versus stretched children; then add a layout driver and observe its final values rather than assuming script assignments remain authoritative. uGUI layout in G11.T01 uses these same rectangles, not an independent world-bounds sizing formula.

## G04.T06 — Transform/physics/render synchronization

`hasChanged` is a manually resettable change indicator in familiar usage, not a comprehensive immutable change history. Hierarchy capacity/count and modern bulk transform methods exist but are not specified here. Transform jobs use a dedicated access API (G17), not general thread-safe access to Transform.

Maintain one authoritative hierarchy while allowing subsystem caches to synchronize at defined boundaries. Physics query geometry may lag direct Transform writes until synchronization; renderer culling bounds may also be cached. Do not assume a world matrix read, physics raycast, and rendered image all observe the same phase merely because called in the same frame. Exact Unity synchronization policy is U015.

Proposed tests (not run): hierarchy world/local round trips; sibling order changes; inactive-parent propagation; RectTransform stretching with different pivots; world-preserving versus local-preserving parenting; rotated non-uniform parent scales; and physics-query visibility before/after explicit sync. The reference supplies no serialized RectTransform layout format or guarantee of matching editor gizmos.
