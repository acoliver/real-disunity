# G07 — 3D physics

**Training-derived, unverified, unofficial; no external source consultation. Training provenance and completeness cannot be certified.** Exact Unity 6.3 behavior is unknown. All code/signatures are authored illustrative subsets from memory, not full declarations or compile-tested examples. See U001–U003 and U022–U024.

**Boundary:** engine Physics module: `UnityEngine.Physics`, `Rigidbody`, `Collider`, contacts and joints. The native 3D solver is historically PhysX-based; this reference specifies neither a solver version nor its full numerical behavior. It is independent of Physics2D.

## G07.T01 — Rigidbody state and forces

Common state: `position`, `rotation`, `mass`, `useGravity`, `isKinematic`, `detectCollisions`, `constraints`, `interpolation`, `collisionDetectionMode`, `centerOfMass`, `worldCenterOfMass`, `inertiaTensor`, and `inertiaTensorRotation`. Unity 6-era `linearVelocity`, `linearDamping`, and `angularDamping` are recalled; older code uses `velocity`, `drag`, and `angularDrag`. Their exact current aliases/obsolescence are U022. `angularVelocity` is in radians per second in familiar 3D behavior.

```csharp
void AddForce(Vector3 force, ForceMode mode);
void AddRelativeForce(Vector3 force, ForceMode mode);
void AddTorque(Vector3 torque, ForceMode mode);
void AddRelativeTorque(Vector3 torque, ForceMode mode);
void AddForceAtPosition(Vector3 force, Vector3 position, ForceMode mode);
void AddExplosionForce(float explosionForce, Vector3 explosionPosition,
                       float explosionRadius, float upwardsModifier, ForceMode mode);
void MovePosition(Vector3 position);
void MoveRotation(Quaternion rotation);
Vector3 GetPointVelocity(Vector3 worldPoint);
void Sleep();
void WakeUp();
bool IsSleeping();
```

`ForceMode.Force`: force integrated over time and mass. `Acceleration`: acceleration independent of mass. `Impulse`: instantaneous momentum change dependent on mass. `VelocityChange`: direct velocity delta independent of mass. Do not multiply an ordinary per-step AddForce by deltaTime again if the solver already integrates it. Applying force away from the center of mass can also produce torque.

Kinematic bodies are moved by the application and influence dynamic bodies without ordinary force integration. Setting body position versus MovePosition has different interpolation/motion semantics. `RigidbodyConstraints` includes axis-specific position/rotation freeze flags and composites `FreezePosition`, `FreezeRotation`, `FreezeAll`, `None`. `RigidbodyInterpolation` includes `None`, `Interpolate`, `Extrapolate`. Collision modes include `Discrete`, `Continuous`, `ContinuousDynamic`, `ContinuousSpeculative`; support and effectiveness depend on body/shape configuration.

## G07.T02 — Collider geometry and materials

Shapes: `BoxCollider` (center, size), `SphereCollider` (center, radius), `CapsuleCollider` (center, radius, height, direction axis), `MeshCollider` (sharedMesh, convex, cooking settings), `TerrainCollider`. `Collider` exposes enabled, isTrigger, attachedRigidbody, bounds, sharedMaterial/material families, `ClosestPoint(Vector3)`, and `Raycast(Ray, out RaycastHit, float)`.

A body's child colliders form a compound shape in familiar hierarchy behavior. Non-convex mesh colliders have body/interaction restrictions; arbitrary concave dynamic meshes should not be accepted just because a mesh can render. Convex cooking is not the original mesh and has vertex/feature limits. Primitive shapes have shape-specific handling under non-uniform scale; bounds remain axis-aligned world bounds.

The physics material type is remembered historically as `PhysicMaterial` and in Unity 6-era naming as `PhysicsMaterial`; exact rename/alias compatibility is explicitly U022. Properties traditionally include `dynamicFriction`, `staticFriction`, `bounciness`, `frictionCombine`, `bounceCombine`. Historical combine enum name is `PhysicMaterialCombine` with `Average`, `Minimum`, `Multiply`, `Maximum`; verify exact Unity 6 spelling instead of inferring a renamed enum. Do not confuse these assets with rendering Material or PhysicsMaterial2D.

## G07.T03 — Queries and filtering

```csharp
// Selected static Physics overload shapes.
static bool Raycast(Vector3 origin, Vector3 direction, out RaycastHit hitInfo,
                    float maxDistance, int layerMask, QueryTriggerInteraction queryTriggerInteraction);
static bool Raycast(Ray ray, out RaycastHit hitInfo, float maxDistance,
                    int layerMask, QueryTriggerInteraction queryTriggerInteraction);
static RaycastHit[] RaycastAll(Ray ray, float maxDistance, int layerMask,
                               QueryTriggerInteraction queryTriggerInteraction);
static int RaycastNonAlloc(Ray ray, RaycastHit[] results, float maxDistance,
                           int layerMask, QueryTriggerInteraction queryTriggerInteraction);
static Collider[] OverlapSphere(Vector3 position, float radius, int layerMask,
                                 QueryTriggerInteraction queryTriggerInteraction);
```

Shorter overloads, bool-only raycasts, SphereCast/BoxCast/CapsuleCast, overlap/check variants, and non-alloc variants form important families. `QueryTriggerInteraction.UseGlobal`, `Ignore`, `Collide` controls trigger inclusion. Global `Physics.queriesHitTriggers` and `queriesHitBackfaces` are relevant. A mask is a bitset, not a layer index. DefaultRaycastLayers and AllLayers are remembered constants with different exclusion policies.

`RaycastHit` includes collider, rigidbody, transform, point, normal, distance, triangleIndex, barycentricCoordinate, textureCoord and related UV/lightmap data, with meaningfulness dependent on collider type and mesh setup. RaycastAll and non-alloc results are not promised sorted. Sort explicitly by distance if the application requires order. A full fixed result buffer can truncate arbitrarily; it is not evidence that no additional hit exists.

Rays starting inside colliders conventionally do not report an entry hit for that collider in common 3D behavior, unlike familiar 2D start-inside handling. Casts with initially overlapping shapes have their own special behavior. `Physics.ComputePenetration` calculates separation between supported collider poses; `ClosestPoint` and `Overlap...` serve different purposes. Exact overloads and degenerate-shape rules are U023.

## G07.T04 — Contacts, callbacks, triggers

Messages: `OnCollisionEnter(Collision)`, `OnCollisionStay(Collision)`, `OnCollisionExit(Collision)`; `OnTriggerEnter(Collider)`, `OnTriggerStay(Collider)`, `OnTriggerExit(Collider)`. A collision pair needs qualifying body/collider participation; two static colliders are not a general event source. Kinematic/static/trigger combinations must be checked against a chosen compatibility matrix.

`Collision` supplies collider/gameObject/transform references, rigidbody, relativeVelocity, impulse, contactCount, and `GetContact(int)`/`GetContacts(...)`. `ContactPoint` supplies point, normal, separation, thisCollider, otherCollider in familiar use. Contact normal orientation depends on which participant observes the event. An exit event need not supply the same meaningful contact manifold as an enter event.

`Physics.IgnoreCollision(Collider, Collider, bool)`, `IgnoreLayerCollision(int,int,bool)`, and getter counterparts alter filtering. Layer masks for queries do not alone configure physical collision response. Newer per-collider layer override and contact modification APIs are intentionally not specified from uncertain recollection. `Physics.reuseCollisionCallbacks` is a remembered allocation-related setting; never retain a transient callback object assuming an immutable snapshot without confirming its lifetime.

## G07.T05 — Joints, CharacterController, vehicles

Joint types include `FixedJoint`, `HingeJoint`, `SpringJoint`, `CharacterJoint`, `ConfigurableJoint`, with base `Joint` properties such as connectedBody, anchor/connectedAnchor, autoConfigureConnectedAnchor, breakForce, breakTorque, enableCollision, and mass scaling. Recalled helper structs: `JointMotor`, `JointLimits`, `JointSpring`, `SoftJointLimit`, `SoftJointLimitSpring`, `JointDrive`. `ConfigurableJointMotion.Locked`, `Limited`, `Free` is common. Drive units and projection behavior require solver-specific treatment, not simplistic interpolation.

`CharacterController` is a specialized capsule movement component, not an ordinary force-driven Rigidbody. `Move(Vector3 motion)` consumes a displacement and returns `CollisionFlags` (`None`, `Sides`, `Above`, `Below`); it does not apply gravity automatically, so callers supply any gravity displacement. In recalled behavior, `SimpleMove(Vector3 speed)` ignores the supplied Y speed, applies gravity automatically, and returns a bool indicating grounding. Neither method turns the controller into an ordinary force-integrated body. `isGrounded` reflects the last move result, not an independent continuous overlap query. Properties include radius, height, center, slopeLimit, stepOffset, skinWidth, minMoveDistance. `OnControllerColliderHit(ControllerColliderHit)` is its collision callback family.

`WheelCollider` supports suspension/tire simulation; familiar fields include motorTorque, brakeTorque, steerAngle, radius, suspensionDistance, suspensionSpring, forwardFriction, sidewaysFriction; `GetWorldPose(out Vector3, out Quaternion)` and `GetGroundHit(out WheelHit)` expose results. It is not a visual wheel mesh or a general convex cylinder collider.

## G07.T06 — Simulation scenes and synchronization

`Physics.gravity` is Vector3. `Physics.Simulate(float step)` manually advances simulation when configured for script control. Modern `Physics.simulationMode` and `SimulationMode.Script` are recalled; older `autoSimulation` usage may be obsolete. `autoSyncTransforms`, `SyncTransforms()`, sleep thresholds, solver iteration counts, and collision settings influence behavior. `PhysicsScene` supports `IsValid`, simulation and query families; scene extension APIs obtain it from a Scene.

Direct Transform changes can be stale to the physics world until synchronization. Moving static geometry frequently can be expensive. Interpolation presents a visual pose distinct from the latest authoritative physics pose. Native solver precision, broadphase policy, contact offsets, continuous collision, mesh cooking, and thread scheduling preclude casual bit-for-bit compatibility (U024).

Proposed tests (not run): each ForceMode's mass/time dependence; compound-collider ownership; trigger/body pairing; unsorted query handling; buffer truncation; ignored collision pairs; CharacterController displacement and grounded state; local-world isolation; and Transform write visibility with explicit sync. This is a selected contract catalog, not a complete PhysX or Unity physics specification.
