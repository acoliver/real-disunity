using Unity.Collections;

namespace UnityEngine;

/// <summary>
/// Unity's 3D physics entry point on an engine without 3D physics simulation. Colliders and
/// rigidbodies cannot be added to a world (<c>AddComponent</c> throws NotSupportedException for them),
/// so a scene never holds a 3D collider and every query returns what Unity returns for such a scene:
/// casts, linecasts and checks return false with a default hit, the *All and Overlap* queries return
/// empty arrays, and the *NonAlloc queries return 0 and leave the caller's buffer untouched
/// (Unity documents the NonAlloc result as the number of entries stored in the buffer).
/// Settings are stored with Unity's defaults and read back; they drive nothing. Simulate and
/// SyncTransforms have nothing to advance or sync. Members that take a collider throw
/// ArgumentNullException for null.
/// </summary>
public partial class Physics
{
    public const int IgnoreRaycastLayer = 4;
    public const int DefaultRaycastLayers = -5;
    public const int AllLayers = -1;

    public delegate void ContactEventDelegate(PhysicsScene scene, NativeArray<ContactPairHeader>.ReadOnly headers);

    /// <summary>Contact callbacks are stored and never raised: the engine produces no 3D contacts.</summary>
    public static event Action<PhysicsScene, NativeArray<ModifiableContactPair>> ContactModifyEvent
    {
        add => _contactModify += value;
        remove => _contactModify -= value;
    }

    public static event Action<PhysicsScene, NativeArray<ModifiableContactPair>> ContactModifyEventCCD
    {
        add => _contactModifyCcd += value;
        remove => _contactModifyCcd -= value;
    }

    public static event ContactEventDelegate ContactEvent
    {
        add => _contactEvent += value;
        remove => _contactEvent -= value;
    }

    private static Action<PhysicsScene, NativeArray<ModifiableContactPair>>? _contactModify;
    private static Action<PhysicsScene, NativeArray<ModifiableContactPair>>? _contactModifyCcd;
    private static ContactEventDelegate? _contactEvent;

    public static Vector3 gravity { get; set; } = new(0f, -9.81f, 0f);
    public static float defaultContactOffset { get; set; } = 0.01f;
    public static float sleepThreshold { get; set; } = 0.005f;
    public static bool queriesHitTriggers { get; set; } = true;
    public static bool queriesHitBackfaces { get; set; }
    public static float bounceThreshold { get; set; } = 2f;
    public static float defaultMaxDepenetrationVelocity { get; set; } = 10f;
    public static int defaultSolverIterations { get; set; } = 6;
    public static int defaultSolverVelocityIterations { get; set; } = 1;
    public static SimulationMode simulationMode { get; set; } = SimulationMode.FixedUpdate;
    public static float defaultMaxAngularSpeed { get; set; } = 7f;
    public static bool improvedPatchFriction { get; set; }
    public static bool invokeCollisionCallbacks { get; set; } = true;
    public static bool reuseCollisionCallbacks { get; set; } = true;
    public static bool autoSyncTransforms { get; set; }
    public static float interCollisionDistance { get; set; }
    public static float interCollisionStiffness { get; set; } = 0.2f;
    public static bool interCollisionSettingsToggle { get; set; }
    public static Vector3 clothGravity { get; set; } = new(0f, -9.81f, 0f);

    /// <summary>The older form of <see cref="simulationMode"/>: true while the mode is FixedUpdate.</summary>
    public static bool autoSimulation
    {
        get => simulationMode == SimulationMode.FixedUpdate;
        set => simulationMode = value ? SimulationMode.FixedUpdate : SimulationMode.Script;
    }

    public static PhysicsScene defaultPhysicsScene => new(PhysicsScene.DefaultHandle);

    public static IntegrationInfo GetCurrentIntegrationInfo() =>
        new(0, "None", "Disunity has no 3D physics simulation", false);

    public static void Simulate(float step) { }

    public static void SyncTransforms() { }

    public static void RebuildBroadphaseRegions(Bounds worldBounds, int subdivisions) { }

    /// <summary>Cooking is a speed-up for creating mesh colliders; with no mesh colliders there is nothing to bake into.</summary>
    public static void BakeMesh(EntityId meshEntityId, bool convex, MeshColliderCookingOptions cookingOptions) { }
    public static void BakeMesh(int meshID, bool convex, MeshColliderCookingOptions cookingOptions) { }
    public static void BakeMesh(int meshID, bool convex) { }
    public static void BakeMesh(EntityId meshEntityId, bool convex) { }

    public static bool ComputePenetration(Collider colliderA, Vector3 positionA, Quaternion rotationA,
        Collider colliderB, Vector3 positionB, Quaternion rotationB, out Vector3 direction, out float distance)
    {
        ArgumentNullException.ThrowIfNull(colliderA);
        ArgumentNullException.ThrowIfNull(colliderB);
        direction = Vector3.zero;
        distance = 0f;
        return false;
    }

    /// <summary>The point on <paramref name="collider"/>, posed at the given position and rotation, closest to <paramref name="point"/>.</summary>
    public static Vector3 ClosestPoint(Vector3 point, Collider collider, Vector3 position, Quaternion rotation)
    {
        ArgumentNullException.ThrowIfNull(collider);
        var local = Quaternion.Inverse(rotation) * (point - position);
        return rotation * collider.ClosestPoint(local) + position;
    }
}
