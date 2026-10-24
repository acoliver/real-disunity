# G06 — 2D physics

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Unity 6.3 is an unverified compatibility target. All signatures/code are authored illustrative subsets from memory, not full declarations or compile-tested examples. See U001–U003, U019–U021.

**Boundary:** native 2D physics module, exposed primarily as `UnityEngine.Physics2D`, `Rigidbody2D`, and `Collider2D`. It is separate from 3D physics. A Collider2D does not collide with a 3D Collider. Matching an API facade does not reproduce the native solver.

## G06.T01 — Bodies and simulation

Important body state: `bodyType` (`RigidbodyType2D.Dynamic`, `Kinematic`, `Static`), `position` (Vector2), `rotation` (float degrees), `mass`, `gravityScale`, `simulated`, `constraints`, `interpolation`, `collisionDetectionMode`, and sleep state. Unity 6-era names `linearVelocity`, `linearDamping`, and `angularDamping` are recalled; historical names include `velocity`, `drag`, and `angularDrag`. Exact alias/obsolescence availability is U019. Angular velocity is conventionally degrees per second for Rigidbody2D, unlike 3D Rigidbody.

```csharp
void AddForce(Vector2 force, ForceMode2D mode);
void AddForceAtPosition(Vector2 force, Vector2 position, ForceMode2D mode);
void AddTorque(float torque, ForceMode2D mode);
void MovePosition(Vector2 position);
void MoveRotation(float angle);
Vector2 GetPointVelocity(Vector2 point);
void Sleep();
void WakeUp();
bool IsSleeping();
```

`ForceMode2D.Force` applies force over simulation time; `Impulse` applies an instantaneous impulse. These are not the four modes of 3D ForceMode. Dynamic bodies integrate forces and respond to contacts. Kinematic bodies are moved explicitly and have different contact response rules; `useFullKinematicContacts` affects which contacts are generated. Static body geometry is not intended as continuously force-integrated motion. `simulated = false` removes participation without destroying the component.

`MovePosition`/`MoveRotation` request physics-step movement, commonly used with kinematic bodies in FixedUpdate. They are not equivalent to assigning Transform state and do not guarantee passage through obstacles or unlimited speed. `RigidbodyConstraints2D` includes `None`, `FreezePositionX`, `FreezePositionY`, `FreezeRotation`, `FreezePosition`, `FreezeAll`. `RigidbodyInterpolation2D` includes `None`, `Interpolate`, `Extrapolate`; `CollisionDetectionMode2D` includes `Discrete`, `Continuous` in remembered use.

## G06.T02 — Colliders and materials

Common types: `BoxCollider2D` (`size`, `edgeRadius`), `CircleCollider2D` (`radius`), `CapsuleCollider2D` (`size`, `direction`), `PolygonCollider2D` (paths), `EdgeCollider2D` (points/edge geometry), `CompositeCollider2D`, and `TilemapCollider2D` in the tilemap module.

Collider2D properties include `enabled`, `isTrigger`, `offset`, `attachedRigidbody`, `bounds`, `sharedMaterial`, `density`, and `usedByEffector`. `PhysicsMaterial2D` supplies friction and bounciness. Automatic mass and collider density interact through body mass settings. Collider world geometry incorporates Transform placement/scale subject to shape-specific restrictions.

Collider2D participates with the nearest applicable Rigidbody2D in the hierarchy in familiar behavior; a collider without an attached body acts as static geometry. Moving a parent Transform, changing scale, or editing polygon paths can require native shape updates. Bounds are an axis-aligned world envelope, not the exact polygon.

Polygon APIs include `pathCount`, `SetPath(int, Vector2[])`, and `GetPath(int)` families; exact list/buffer alternatives are U020. Composite setup has changed over Unity generations: historical `usedByComposite` and newer composition operations must not be treated as interchangeable certified 6.3 members. Holes, winding, vertex limits, self-intersections, and tessellation require geometry-specific validation.

## G06.T03 — Queries and filtering

```csharp
// Selected static Physics2D query shapes, explicit arguments shown.
static RaycastHit2D Raycast(Vector2 origin, Vector2 direction, float distance,
                           int layerMask, float minDepth, float maxDepth);
static RaycastHit2D[] RaycastAll(Vector2 origin, Vector2 direction, float distance,
                                int layerMask, float minDepth, float maxDepth);
static Collider2D OverlapPoint(Vector2 point, int layerMask, float minDepth, float maxDepth);
static Collider2D OverlapCircle(Vector2 point, float radius, int layerMask,
                                float minDepth, float maxDepth);
static RaycastHit2D GetRayIntersection(Ray ray, float distance, int layerMask);
```

Shorter overloads commonly supply default distances/masks/depth bounds. Box/Circle/Capsule casts, Linecast, overlap shapes, All variants, and caller-supplied results families also exist. `ContactFilter2D` allows explicit layer, depth, trigger, and normal-angle filtering where the query supports them; overlap queries do not use normal-angle filtering in recalled behavior. Filtered Raycast forms accepting a result array or list are important. Exact signatures and legacy `...NonAlloc` status are U020 rather than exhaustively invented here.

`RaycastHit2D` includes `collider`, `rigidbody`, `transform`, `point`, `normal`, `distance`, `fraction`, and centroid-related state. A hit can be checked through its collider or remembered boolean conversion. All-hit query results have API-specific ordering; do not assume a filled buffer is complete or universally sorted. Caller-buffer APIs return a count; entries beyond that count are not results. A full buffer can mean truncation.

2D depth filtering uses the Z placement of otherwise XY collision geometry. Angles for oriented shape queries are degrees. `Physics2D.queriesHitTriggers` and `queriesStartInColliders` affect familiar query behavior. A ray beginning within a collider can report an initial hit with special normal/fraction semantics; review before treating that as a conventional surface entry. Use explicit contact filters to avoid hidden project-global policy when possible.

## G06.T04 — Collision and trigger events

Message shapes: `OnCollisionEnter2D(Collision2D)`, `OnCollisionStay2D(Collision2D)`, `OnCollisionExit2D(Collision2D)`; trigger counterparts take `Collider2D`. Contacts and triggers are emitted from physics simulation, not one callback per render frame. Multiple colliders can produce multiple pair events for the same pair of GameObjects.

`Collision2D` exposes colliders/body references, `relativeVelocity`, `contactCount`, and contact retrieval (`GetContact`, `GetContacts` families). `ContactPoint2D` includes point, normal, separation, impulse-related values, and involved colliders in remembered APIs. Prefer reusable contact buffers where allocation matters. Collision contacts, solver response, and trigger overlap are distinct observations.

A trigger requires a qualifying body/collider pair; two arbitrary static shapes are not promised to emit all expected callbacks. Disabled MonoBehaviours can receive certain physics messages historically, so applying the ordinary Update-enabled filter to all collision callbacks is unsafe. Exact enter/exit delivery after deactivation, destruction, body-type changes, and collider removal is U021.

`Physics2D.IgnoreCollision(Collider2D, Collider2D, bool)` and `IgnoreLayerCollision(int,int,bool)` change pair/layer interaction. Pair ignore state lifecycle and interaction with trigger events should be tested. The layer collision matrix and query mask are different filtering stages.

## G06.T05 — Joints and effectors

Representative joints: `DistanceJoint2D`, `FixedJoint2D`, `FrictionJoint2D`, `HingeJoint2D`, `RelativeJoint2D`, `SliderJoint2D`, `SpringJoint2D`, `TargetJoint2D`, `WheelJoint2D`. Base `Joint2D` supplies connected-body and break concepts; anchored joints supply anchor/connectedAnchor and auto-configuration where applicable.

Remembered data types: `JointMotor2D` (`motorSpeed`, `maxMotorTorque`), `JointAngleLimits2D` (`min`, `max`), and `JointTranslationLimits2D` (`min`, `max`). Both HingeJoint2D.motor and SliderJoint2D.motor use JointMotor2D in the recalled surface. Despite the field name maxMotorTorque, the slider interprets the limit as motor force and motorSpeed as linear speed, unlike the hinge's angular motor. Do not invent a separate slider motor struct or a maxMotorForce field on JointMotor2D. Springs use frequency/damping-related settings; solver behavior is not an analytic spring update.

`PlatformEffector2D` models one-way/platform contact rules; `AreaEffector2D`, `PointEffector2D`, `SurfaceEffector2D`, and `BuoyancyEffector2D` apply specialized interactions. Colliders must be configured to use effectors as appropriate. Surface normal thresholds, one-way grouping, force application modes, and interpolation details are not exhaustively specified.

## G06.T06 — Stepping, scenes, numerical compatibility

`Physics2D.gravity` is Vector2. `simulationMode` uses remembered `SimulationMode2D.FixedUpdate`, `Update`, and `Script`; exact current options are U019. `Physics2D.Simulate(float step)` and `SyncTransforms()` provide explicit stepping/synchronization. Scene-local `PhysicsScene2D` is obtained through scene extension APIs and supports simulation/queries. A local physics scene is not automatically advanced merely because the default world steps.

A subset implementation should choose a solver, units, fixed-step policy, broadphase, shape tolerances, and event-pair tracking. Unity-compatible names do not establish Box2D-version equivalence or deterministic replay. Avoid promising bitwise cross-platform results, exact stacking behavior, continuous-collision equivalence, or native contact ordering.

Proposed tests (not executed): constant-force integration versus impulse; XY-only contacts; layer and depth filtering; trigger query policy; ray origin inside a shape; limited result-buffer count/truncation; sleeping/waking; interpolation versus authoritative position; explicit synchronization after Transform changes; and isolation of local physics scenes. Native shape serialization, composite tessellation, contact reuse options, and complete per-layer override rules remain gaps.
