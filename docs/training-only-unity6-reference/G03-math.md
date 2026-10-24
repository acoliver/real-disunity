# G03 — Mathematics and value types

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6.3 is a desired, unverified target. Code/signatures are illustrative authored subsets from memory, not complete declarations or compile-tested examples. See U001–U003 and U010–U012.

**Boundary:** engine `UnityEngine` value types. The separately versioned `Unity.Mathematics` package (`float3`, `quaternion`, `float4x4`, `math`) is related but has different types/operators and is not a drop-in source-compatible replacement.

## G03.T01 — Scalar math and Mathf

`Mathf` provides float-oriented scalar functions. Common constants: `PI`, `Deg2Rad`, `Rad2Deg`, `Infinity`, `NegativeInfinity`, `Epsilon`. Epsilon is a tiny representable threshold, not a universal geometric tolerance.

Selected families:

```csharp
static float Abs(float f);
static float Min(float a, float b);
static float Max(float a, float b);
static float Clamp(float value, float min, float max);
static int Clamp(int value, int min, int max);
static float Clamp01(float value);
static float Lerp(float a, float b, float t);
static float LerpUnclamped(float a, float b, float t);
static float InverseLerp(float a, float b, float value);
static float MoveTowards(float current, float target, float maxDelta);
static float SmoothDamp(float current, float target, ref float currentVelocity,
                        float smoothTime, float maxSpeed, float deltaTime);
```

Min/Max have integer and params-array overload families; Abs has scalar float and int forms, not an array overload in the recalled surface. Familiar functions include `Sin`, `Cos`, `Tan`, `Asin`, `Acos`, `Atan`, `Atan2`, `Sqrt`, `Pow`, `Exp`, `Log`, `Log10`, `Floor`, `Ceil`, `Round`, corresponding `...ToInt` conversions, `Sign`, `Approximately`, `Repeat`, `PingPong`, `DeltaAngle`, `LerpAngle`, `MoveTowardsAngle`, `SmoothDampAngle`, `SmoothStep`, `PerlinNoise`, `ClosestPowerOfTwo`, `NextPowerOfTwo`, and `IsPowerOfTwo`.

Trig inputs are radians, while many rotation APIs use degrees. Lerp clamps t in ordinary Unity scalar/vector use; LerpUnclamped does not. InverseLerp conventionally clamps and needs defined handling when endpoints coincide. MoveTowards limits step length, unlike Lerp's fraction. SmoothDamp mutates the velocity parameter and is a damped approach, not a fixed-duration animation. Its shorter overloads supply limits/time from defaults; exact defaults are not reproduced.

`DeltaAngle` wraps to a short signed angular difference. `Repeat` should not be casually equated with C# remainder for negative inputs. Recalled rounding uses nearest with tie-to-even behavior in common Mathf conversions; negative ties, overflow, NaN, and Infinity are U010. `Approximately` is not an absolute epsilon-only comparison.

## G03.T02 — Vectors and integer vectors

`Vector2`, `Vector3`, `Vector4` have public float components, constructors, indexers, arithmetic operators, magnitude/sqrMagnitude, normalized forms, and static direction/zero/one values where appropriate. `Vector2Int` and `Vector3Int` have integer components and discrete-grid semantics; normalization does not become an integer direction API.

```csharp
// Representative Vector3 operations.
static float Dot(Vector3 lhs, Vector3 rhs);
static Vector3 Cross(Vector3 lhs, Vector3 rhs);
static float Distance(Vector3 a, Vector3 b);
static float Angle(Vector3 from, Vector3 to);
static float SignedAngle(Vector3 from, Vector3 to, Vector3 axis);
static Vector3 Lerp(Vector3 a, Vector3 b, float t);
static Vector3 Slerp(Vector3 a, Vector3 b, float t);
static Vector3 MoveTowards(Vector3 current, Vector3 target, float maxDistanceDelta);
static Vector3 Project(Vector3 vector, Vector3 onNormal);
static Vector3 ProjectOnPlane(Vector3 vector, Vector3 planeNormal);
static Vector3 Reflect(Vector3 inDirection, Vector3 inNormal);
static Vector3 Scale(Vector3 a, Vector3 b);
void Normalize();
```

`Vector3.forward` is +Z, `up` is +Y, `right` is +X. `Vector2` uses XY. `Vector3 * Vector3` is not the standard componentwise operation; use Scale. Dot returns a scalar; Cross returns an oriented perpendicular. Cross order matters. Vector Angle returns degrees. Normalize of a sufficiently small vector conventionally yields zero rather than a useful direction; exact threshold is not specified.

Vector operators `==` commonly use approximate geometric equality whereas `Equals` has exact component semantics in recalled Unity types. This distinction affects hash containers and must be verified type-by-type (U010). Do not define hash keys by epsilon equality without a coherent hash contract. Vector4, integer vectors, and quaternion equality should not be assumed to share one implementation.

Implicit/explicit conversion families between vectors exist; conversions may discard components or introduce zero components. A compatibility implementation must reproduce actual conversion operators rather than adding every plausible one.

## G03.T03 — Quaternions

`Quaternion` stores `x`, `y`, `z`, `w`; identity is `(0,0,0,1)`, not all zeros. A unit quaternion represents a rotation; q and -q represent the same orientation.

```csharp
static Quaternion Euler(float x, float y, float z);
static Quaternion Euler(Vector3 euler);
static Quaternion AngleAxis(float angle, Vector3 axis);
static Quaternion LookRotation(Vector3 forward, Vector3 upwards);
static Quaternion FromToRotation(Vector3 fromDirection, Vector3 toDirection);
static Quaternion Inverse(Quaternion rotation);
static Quaternion Slerp(Quaternion a, Quaternion b, float t);
static Quaternion Lerp(Quaternion a, Quaternion b, float t);
static Quaternion RotateTowards(Quaternion from, Quaternion to, float maxDegreesDelta);
static float Angle(Quaternion a, Quaternion b);
void ToAngleAxis(out float angle, out Vector3 axis);
Vector3 eulerAngles { get; set; }
```

Quaternion multiplication composes rotations; `q * v` rotates a Vector3. Order is noncommutative. For Transform hierarchy reasoning, world rotation is parent world rotation multiplied by local rotation. Euler inputs/outputs are degrees. Unity's remembered Euler application convention is Z, then X, then Y; exact intrinsic/extrinsic wording and singularity behavior require U011 review. A displayed Euler triple is not a unique canonical rotation and can wrap/discontinue.

Equivalent orientation does not imply identical components or identical operator/Equals results (J3-018). Recalled Quaternion operator equality uses a near-positive-unit dot-product criterion: q and -q should not be assumed to compare equal although they rotate vectors identically. For unit rotations, use an orientation comparison such as `Quaternion.Angle(a, b)` against an application-chosen angular tolerance. Equals/hash keys instead have a component-value contract; do not use orientation equivalence as hash equality without a consistent representation policy. Exact tolerances and per-type behavior remain U010/U011.

Slerp follows a spherical orientation interpolation; Lerp is normalized in familiar Unity quaternion use and is not component interpolation without renormalization. Unclamped variants exist. LookRotation's default up is a familiar shorter overload; zero/collinear input behavior and warning/identity fallback are version-sensitive. Non-normalized quaternion input is not a safe general contract. Do not assert bitwise agreement with a different math library.

## G03.T04 — Matrices and projection math

`Matrix4x4` has fields `m00` through `m33`, indexers, `identity`, `zero`, `inverse`, `transpose`, `determinant`, and operations such as:

```csharp
static Matrix4x4 TRS(Vector3 pos, Quaternion q, Vector3 s);
static Matrix4x4 Translate(Vector3 vector);
static Matrix4x4 Rotate(Quaternion q);
static Matrix4x4 Scale(Vector3 vector);
static Matrix4x4 Perspective(float fov, float aspect, float zNear, float zFar);
static Matrix4x4 Ortho(float left, float right, float bottom, float top, float zNear, float zFar);
Vector3 MultiplyPoint(Vector3 point);
Vector3 MultiplyPoint3x4(Vector3 point);
Vector3 MultiplyVector(Vector3 vector);
Vector4 GetColumn(int index);
Vector4 GetRow(int index);
```

Unity uses column-vector style transform reasoning: M * v; a TRS maps a local point by scale, rotation, then translation. Memory layout is column-major in the familiar engine representation, which is distinct from the semantic meaning of m[row,column]. MultiplyPoint includes projective division; MultiplyPoint3x4 assumes an affine matrix; MultiplyVector excludes translation. Singular inversion and decomposition are not fully specified here.

A camera projection matrix is not automatically the exact GPU projection for all backends. `GL.GetGPUProjectionMatrix(...)`, reversed-Z, render-target orientation, clipping ranges, and stereo paths belong to rendering. A non-uniform-scale hierarchy can introduce shear not representable by a single TRS. Extracting rotation/scale is consequently lossy or ambiguous.

## G03.T05 — Geometric value types

- `Ray(Vector3 origin, Vector3 direction)` exposes origin, direction, `GetPoint(float distance)`. In recalled engine behavior, the constructor and direction setter normalize the supplied direction; GetPoint uses the stored direction. A zero direction remains degenerate. Do not infer the same normalization contract for every physics overload that accepts a separate direction vector.
- `Plane` has normal/distance forms, construction from points, `GetDistanceToPoint`, `GetSide`, `ClosestPointOnPlane`, and `Raycast(Ray, out float enter)`. Plane equation uses normal·point + distance = 0 in recalled semantics.
- `Bounds(Vector3 center, Vector3 size)` is axis-aligned, with center/size/extents/min/max, `Contains`, `Intersects`, `Encapsulate`, `Expand`, `ClosestPoint`, and ray intersection families. Size is full span; extents are half span. It is not an oriented box.
- `Rect` uses x/y/width/height and xMin/yMin/xMax/yMax, position/size/center/min/max, `Contains` and `Overlaps`. Screen-space origin convention belongs to the consumer, not Rect itself.
- `BoundsInt` and `RectInt` represent integer regions; Vector2Int/Vector3Int represent coordinates, not region iterators. Use `RectInt.allPositionsWithin` and `BoundsInt.allPositionsWithin` to enumerate their regions.
- `Color` is float RGBA, `Color32` byte RGBA. Named colors, `Lerp`, `HSVToRGB`, `RGBToHSV`, `.linear`, `.gamma`, and `ColorUtility.TryParseHtmlString` are common. Values can be outside 0–1 in HDR float workflows; conversion to Color32 necessarily loses range/precision.

### Ordinary containment boundaries (J3-003)

Recalled baseline for positive-size regions, not an exact-version verification:

| Operation / region | Minimum boundary | Maximum boundary |
|---|---|---|
| Rect.Contains(point), ordinary positive width/height | Included on X/Y | Excluded on X/Y |
| Bounds.Contains(point), nonnegative extents | Included | Included, including faces/edges/corners |
| RectInt.Contains / allPositionsWithin | Included | Excluded |
| BoundsInt.Contains / allPositionsWithin | Included | Excluded |

For integer regions these are coordinate ranges, not floating boxes centered on grid cells. Negative coordinates do not change inclusion: a RectInt with minimum (-2,-1), size (3,2) enumerates X = -2,-1,0 and Y = -1,0. A Bounds with negative extents is not an ordinarily valid containing bounds; recalled Contains does not accept points in such a bounds. Rect overloads taking `allowInverse` explicitly address inverse rectangles; do not silently apply that behavior to the ordinary Contains overload. Contains tests a point; Overlaps/Intersects test relationships between regions and must not inherit point-edge rules by analogy. Zero sizes, inverse integer regions, NaN and overload-specific overlap behavior remain U012.

Proposed cases (not run): each minimum corner, maximum corner, each boundary face/edge and adjacent outside point; negative-coordinate regions; compare enumerated coordinates with integer Contains; separately test inverse Rect overloads and invalid Bounds extents.

## G03.T06 — Randomness, interpolation, numerical contracts

`UnityEngine.Random` has global state through `Random.state`, seeding with `InitState(int)`, `value`, `Range(int,int)`, `Range(float,float)`, `insideUnitCircle`, `insideUnitSphere`, `onUnitSphere`, and rotation/color helpers. Integer Range is conventionally minimum-inclusive, maximum-exclusive; float Range can include both endpoints. Equal/reversed bounds and exact generator sequences require verification. Global state means unrelated consumers can perturb reproducibility. Do not substitute `System.Random` and promise identical streams.

Proposed tests (not run): TRS of a known point, parent/local quaternion composition, zero normalization, Lerp clamping versus unclamped interpolation, integer random bounds, normal-versus-point transformation under scale, and bounds/rect edge behavior. Use tolerances selected for the physical quantity and operation; do not reuse Mathf.Epsilon indiscriminately. NaN propagation, denormal handling, SIMD/Burst differences, transcendental precision, and bitwise cross-platform determinism remain outside the certified scope.
