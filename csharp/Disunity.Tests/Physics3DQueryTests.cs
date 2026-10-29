using System.Reflection;
using UnityEngine;

/// <summary>Physics queries in a world with no 3D colliders, the physics settings, layer rules and RaycastHit.</summary>
internal static class Physics3DQueryTests
{
    private static readonly BindingFlags Public = BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly;

    public static void Run()
    {
        EveryQueryGivesTheEmptyWorldAnswer();
        QueriesOnOtherTypes();
        SettingsDefaultsAndRoundTrips();
        LayerCollisionMatrix();
        ColliderPairRules();
        PenetrationAndClosestPoint();
        PhysicsScenes();
        RaycastHits();
        Console.WriteLine("PASS: 3D physics queries answer for an empty world, settings, layer rules, scenes and RaycastHit behave as specified");
    }

    private static object? Argument(Type type)
    {
        if (type.IsByRef) type = type.GetElementType()!;
        if (type == typeof(Vector3)) return new Vector3(1, 2, 3);
        if (type == typeof(Quaternion)) return Quaternion.identity;
        if (type == typeof(Ray)) return new Ray(Vector3.zero, Vector3.forward);
        if (type == typeof(float)) return 5f;
        if (type == typeof(int)) return -1;
        if (type == typeof(QueryTriggerInteraction)) return QueryTriggerInteraction.UseGlobal;
        if (type == typeof(RaycastHit[])) return Enumerable.Repeat(Marked, 3).ToArray();
        if (type == typeof(Collider[])) return Enumerable.Repeat((Collider)new BoxCollider(), 3).ToArray();
        return type.IsValueType ? Activator.CreateInstance(type) : null;
    }

    private static RaycastHit Marked
    {
        get
        {
            var hit = new RaycastHit();
            hit.distance = 7f;
            return hit;
        }
    }

    private static int Check(object? target, MethodInfo method)
    {
        var parameters = method.GetParameters();
        var args = parameters.Select(p => Argument(p.ParameterType)).ToArray();
        var snapshots = args.Select(a => a is Array array ? array.Cast<object>().ToArray() : null).ToArray();
        var result = method.Invoke(target, args);
        var name = $"{method.DeclaringType!.Name}.{method.Name}({string.Join(", ", parameters.Select(p => p.ParameterType.Name))})";
        Expect.That(result switch { bool b => !b, int n => n == 0, Array a => a.Length == 0, _ => false }, $"{name} answers for an empty world, got {result}");
        for (var i = 0; i < args.Length; i++)
        {
            if (snapshots[i] is { } before) Expect.That(((Array)args[i]!).Cast<object>().SequenceEqual(before), $"{name} leaves its buffer untouched");
            if (parameters[i].ParameterType.IsByRef) Expect.That(Equals(args[i], Activator.CreateInstance(parameters[i].ParameterType.GetElementType()!)), $"{name} clears its out parameter");
        }
        return 1;
    }

    private static readonly HashSet<string> QueryFamilies = new(
        ["Raycast", "Linecast", "SphereCast", "BoxCast", "CapsuleCast", "RaycastAll", "SphereCastAll", "BoxCastAll", "CapsuleCastAll",
         "RaycastNonAlloc", "SphereCastNonAlloc", "BoxCastNonAlloc", "CapsuleCastNonAlloc", "OverlapSphere", "OverlapBox", "OverlapCapsule",
         "OverlapSphereNonAlloc", "OverlapBoxNonAlloc", "OverlapCapsuleNonAlloc", "CheckSphere", "CheckBox", "CheckCapsule"]);

    private static void EveryQueryGivesTheEmptyWorldAnswer()
    {
        var methods = typeof(Physics).GetMethods(Public).Where(m => m.IsStatic && QueryFamilies.Contains(m.Name)).ToList();
        Expect.That(methods.Count == 132, $"Physics has 132 query overloads, found {methods.Count}");
        var checkedCount = methods.Sum(method => Check(null, method));
        var scene = (object)Physics.defaultPhysicsScene;
        var sceneMethods = typeof(PhysicsScene).GetMethods(Public).Where(m => m.GetParameters().Length > 2 && m.Name is not "RunSimulationStages").ToList();
        Expect.That(sceneMethods.Count == 15, $"PhysicsScene has 15 query overloads, found {sceneMethods.Count}");
        checkedCount += sceneMethods.Sum(method => Check(scene, method));
        Expect.That(checkedCount == 147, $"checked {checkedCount} overloads");
    }

    private static void QueriesOnOtherTypes()
    {
        Expect.That(!Physics.Raycast(Vector3.zero, Vector3.down, out var hit, 100f) && hit.collider == null, "typical script raycast");
        Expect.That(Physics.OverlapSphere(Vector3.zero, 10f).Length == 0, "typical overlap sphere");
        Expect.That(!Physics.CheckSphere(Vector3.zero, 1f, Physics.AllLayers, QueryTriggerInteraction.Collide), "check sphere with layer mask");
        var buffer = new Collider[2] { new SphereCollider(), new SphereCollider() };
        var first = buffer[0];
        Expect.That(Physics.OverlapSphereNonAlloc(Vector3.zero, 10f, buffer) == 0 && ReferenceEquals(buffer[0], first), "NonAlloc leaves the buffer");
    }

    private static void SettingsDefaultsAndRoundTrips()
    {
        Expect.That(Physics.gravity == new Vector3(0, -9.81f, 0), "gravity default");
        Expect.That(Physics.defaultContactOffset == 0.01f && Physics.sleepThreshold == 0.005f && Physics.bounceThreshold == 2f, "contact, sleep and bounce defaults");
        Expect.That(Physics.defaultSolverIterations == 6 && Physics.defaultSolverVelocityIterations == 1, "solver defaults");
        Expect.That(Physics.queriesHitTriggers && !Physics.queriesHitBackfaces && Physics.invokeCollisionCallbacks && Physics.reuseCollisionCallbacks, "query and callback defaults");
        Expect.That(Physics.simulationMode == SimulationMode.FixedUpdate && Physics.autoSimulation && !Physics.autoSyncTransforms, "simulation defaults");
        Expect.That(Physics.defaultMaxAngularSpeed == 7f && Physics.defaultMaxDepenetrationVelocity == 10f && !Physics.improvedPatchFriction, "angular and depenetration defaults");
        Expect.That(Physics.clothGravity == new Vector3(0, -9.81f, 0) && Physics.interCollisionStiffness == 0.2f, "cloth defaults");
        Physics.gravity = new Vector3(0, -20, 0);
        Expect.That(Physics.gravity == new Vector3(0, -20, 0), "gravity round trip");
        Physics.gravity = new Vector3(0, -9.81f, 0);
        Physics.queriesHitTriggers = false;
        Expect.That(!Physics.queriesHitTriggers, "queriesHitTriggers round trip");
        Physics.queriesHitTriggers = true;
        Physics.autoSimulation = false;
        Expect.That(Physics.simulationMode == SimulationMode.Script, "autoSimulation false selects Script mode");
        Physics.simulationMode = SimulationMode.Update;
        Expect.That(!Physics.autoSimulation, "only FixedUpdate is auto simulation");
        Physics.autoSimulation = true;
        Expect.That(Physics.simulationMode == SimulationMode.FixedUpdate, "autoSimulation true selects FixedUpdate");
        Physics.Simulate(0.02f);
        Physics.SyncTransforms();
        Expect.That(Physics.gravity == new Vector3(0, -9.81f, 0), "Simulate changes nothing");
        var info = Physics.GetCurrentIntegrationInfo();
        Expect.That(info.name.Length > 0 && info.description.Length > 0, "integration info describes the integration");
    }

    private static void LayerCollisionMatrix()
    {
        Expect.That(!Physics.GetIgnoreLayerCollision(0, 8) && !Physics.GetIgnoreLayerCollision(31, 31), "layers collide by default");
        Physics.IgnoreLayerCollision(0, 8);
        Expect.That(Physics.GetIgnoreLayerCollision(0, 8) && Physics.GetIgnoreLayerCollision(8, 0), "ignoring is symmetric");
        Expect.That(!Physics.GetIgnoreLayerCollision(0, 9), "other pairs are untouched");
        Physics.IgnoreLayerCollision(8, 0, false);
        Expect.That(!Physics.GetIgnoreLayerCollision(0, 8) && !Physics.GetIgnoreLayerCollision(8, 0), "clearing is symmetric");
        Physics.IgnoreLayerCollision(31, 31, true);
        Expect.That(Physics.GetIgnoreLayerCollision(31, 31), "a layer can ignore itself");
        Physics.IgnoreLayerCollision(31, 31, false);
        foreach (var bad in new[] { -1, 32 })
        {
            Expect.Throws<ArgumentOutOfRangeException>(() => Physics.IgnoreLayerCollision(bad, 0, true), $"IgnoreLayerCollision layer1 {bad}");
            Expect.Throws<ArgumentOutOfRangeException>(() => Physics.IgnoreLayerCollision(0, bad), $"IgnoreLayerCollision layer2 {bad}");
            Expect.Throws<ArgumentOutOfRangeException>(() => Physics.GetIgnoreLayerCollision(bad, 0), $"GetIgnoreLayerCollision {bad}");
        }
        Expect.That(!Physics.GetIgnoreLayerCollision(0, 31), "a rejected call changes nothing");
    }

    private static void ColliderPairRules()
    {
        Expect.That(Physics.DefaultRaycastLayers == ~Physics.IgnoreRaycastLayer, "default raycast layers exclude the Ignore Raycast layer mask");
        var a = new BoxCollider();
        var b = new SphereCollider();
        Expect.That(!Physics.GetIgnoreCollision(a, b), "colliders collide by default");
        Physics.IgnoreCollision(a, b);
        Expect.That(Physics.GetIgnoreCollision(a, b) && Physics.GetIgnoreCollision(b, a), "collider pairs are symmetric");
        Physics.IgnoreCollision(b, a, false);
        Expect.That(!Physics.GetIgnoreCollision(a, b), "collider pair cleared");
        Expect.Throws<ArgumentNullException>(() => Physics.IgnoreCollision(null!, b), "IgnoreCollision null first");
        Expect.Throws<ArgumentNullException>(() => Physics.IgnoreCollision(a, null!, true), "IgnoreCollision null second");
        Expect.Throws<ArgumentNullException>(() => Physics.GetIgnoreCollision(a, null!), "GetIgnoreCollision null");
    }

    private static void PenetrationAndClosestPoint()
    {
        var a = new BoxCollider();
        var b = new BoxCollider();
        var direction = new Vector3(9, 9, 9);
        var distance = 9f;
        Expect.That(!Physics.ComputePenetration(a, Vector3.zero, Quaternion.identity, b, Vector3.zero, Quaternion.identity, out direction, out distance), "no penetration is computed");
        Expect.That(direction == Vector3.zero && distance == 0f, "penetration outputs are cleared");
        Expect.Throws<ArgumentNullException>(() => Physics.ComputePenetration(null!, Vector3.zero, Quaternion.identity, b, Vector3.zero, Quaternion.identity, out _, out _), "ComputePenetration null A");
        Expect.Throws<ArgumentNullException>(() => Physics.ComputePenetration(a, Vector3.zero, Quaternion.identity, null!, Vector3.zero, Quaternion.identity, out _, out _), "ComputePenetration null B");
        var posed = Physics.ClosestPoint(new Vector3(13, 0, 0), a, new Vector3(10, 0, 0), Quaternion.identity);
        Expect.That(posed == new Vector3(10.5f, 0, 0), $"closest point on a posed box, got {posed}");
        var turned = Physics.ClosestPoint(new Vector3(0, 0, -3), new BoxCollider { size = new Vector3(2, 2, 6) }, Vector3.zero, Quaternion.Euler(0, 90, 0));
        Expect.That(turned == new Vector3(0, 0, -1), $"closest point on a box turned a quarter, got {turned}");
        Expect.Throws<ArgumentNullException>(() => Physics.ClosestPoint(Vector3.zero, null!, Vector3.zero, Quaternion.identity), "ClosestPoint null collider");
    }

    private static void PhysicsScenes()
    {
        var scene = Physics.defaultPhysicsScene;
        Expect.That(scene.IsValid() && scene.IsEmpty(), "the default scene is valid and empty");
        Expect.That(scene == Physics.defaultPhysicsScene && scene.Equals((object)Physics.defaultPhysicsScene), "scenes compare by handle");
        var none = default(PhysicsScene);
        Expect.That(!none.IsValid() && none != scene, "a default scene is invalid");
        Expect.Throws<InvalidOperationException>(() => none.IsEmpty(), "IsEmpty on an invalid scene");
        scene.Simulate(0.02f);
        scene.InterpolateBodies();
        Expect.That(scene.Raycast(Vector3.zero, Vector3.down, 10f, -5, QueryTriggerInteraction.UseGlobal) == false, "scene raycast");
    }

    private static void RaycastHits()
    {
        var hit = new RaycastHit();
        Expect.That(hit.collider == null && hit.transform == null && hit.rigidbody == null && hit.articulationBody == null, "default hit has no objects");
        Expect.That(hit.point == Vector3.zero && hit.normal == Vector3.zero && hit.distance == 0f && hit.triangleIndex == 0, "default hit values");
        Expect.That(hit.colliderInstanceID == 0 && !hit.colliderEntityId.IsValid(), "default hit has no collider id");
        hit.point = new Vector3(1, 2, 3);
        hit.normal = Vector3.up;
        hit.distance = 4.5f;
        Expect.That(hit.point == new Vector3(1, 2, 3) && hit.normal == Vector3.up && hit.distance == 4.5f, "hit setters");
        hit.barycentricCoordinate = new Vector3(0.5f, 0.25f, 0.25f);
        Expect.That(hit.barycentricCoordinate == new Vector3(0.5f, 0.25f, 0.25f) && hit.textureCoord == new Vector2(0.25f, 0.25f), "barycentric coordinate round trip");
        Expect.That(hit.textureCoord2 == Vector2.zero && hit.lightmapCoord == Vector2.zero, "unused coordinates are zero");
    }
}
