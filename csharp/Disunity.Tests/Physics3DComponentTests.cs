using System.Reflection;
using Disunity.Hosting;
using UnityEngine;

/// <summary>The 3D physics components: AddComponent is refused, and free-standing instances hold Unity's defaults.</summary>
internal static class Physics3DComponentTests
{
    public static void Run()
    {
        AddComponentIsRefused();
        TypeShapes();
        ColliderDefaultsAndGeometry();
        ColliderMaterials();
        RigidbodyState();
        CollisionData();
        Console.WriteLine("PASS: 3D physics components cannot be added, and their types, defaults, geometry and collision data match Unity");
    }

    private static void Refused<T>(GameObject go) where T : Component, new()
    {
        var error = Expect3D<T>(go);
        Expect.That(error.Message == $"3D physics components are not supported yet: {typeof(T).Name}", $"message for {typeof(T).Name}: {error.Message}");
        Expect.That(go.GetComponent<T>() == null, $"{typeof(T).Name} stays off the GameObject");
    }

    private static NotSupportedException Expect3D<T>(GameObject go) where T : Component, new()
    {
        try { go.AddComponent<T>(); }
        catch (NotSupportedException error) { return error; }
        throw new InvalidOperationException($"AddComponent<{typeof(T).Name}> did not throw");
    }

    private static void AddComponentIsRefused()
    {
        DisunityHost.LoadWorld();
        try
        {
            var go = DisunityHost.CreateGameObject();
            Refused<Collider>(go);
            Refused<BoxCollider>(go);
            Refused<SphereCollider>(go);
            Refused<CapsuleCollider>(go);
            Refused<MeshCollider>(go);
            Refused<Rigidbody>(go);
            Refused<ArticulationBody>(go);
            var body = go.AddComponent<Rigidbody2D>();
            var box = go.AddComponent<BoxCollider2D>();
            Expect.That(go.GetComponent<Rigidbody2D>() == body && go.GetComponent<BoxCollider2D>() == box, "2D physics components still attach");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void TypeShapes()
    {
        Expect.That(typeof(Collider).BaseType == typeof(Component), "Collider is a Component");
        foreach (var shape in new[] { typeof(BoxCollider), typeof(SphereCollider), typeof(CapsuleCollider), typeof(MeshCollider) })
            Expect.That(shape.BaseType == typeof(Collider), $"{shape.Name} derives from Collider");
        Expect.That(typeof(Rigidbody).BaseType == typeof(Component), "Rigidbody is a Component");
        Expect.That(typeof(ArticulationBody).BaseType == typeof(Behaviour), "ArticulationBody is a Behaviour");
        Expect.That(typeof(Physics).IsClass && !typeof(Physics).IsAbstract, "Physics is a plain class, as in Unity");
        Expect.That(typeof(RaycastHit).IsValueType && typeof(Collision).IsClass, "RaycastHit is a struct and Collision a class");
        Expect.Equal(5, (int)ForceMode.Acceleration, "ForceMode.Acceleration");
        Expect.Equal(126, (int)RigidbodyConstraints.FreezeAll, "RigidbodyConstraints.FreezeAll");
        Expect.Equal(-5, Physics.DefaultRaycastLayers, "DefaultRaycastLayers");
        var hitFields = typeof(RaycastHit).GetFields(BindingFlags.Instance | BindingFlags.NonPublic).Select(f => f.Name);
        Expect.Sequence(["m_Point", "m_Normal", "m_FaceID", "m_Distance", "m_UV", "m_Collider"], hitFields, "RaycastHit field layout");
    }

    private static void ColliderDefaultsAndGeometry()
    {
        var box = new BoxCollider();
        Expect.That(box.enabled && !box.isTrigger && box.contactOffset == 0.01f, "collider defaults");
        Expect.That(box.size == Vector3.one && box.center == Vector3.zero, "box defaults");
        box.center = new Vector3(1, 2, 3);
        box.size = new Vector3(2, 4, 6);
        Expect.That(box.bounds.center == new Vector3(1, 2, 3) && box.bounds.size == new Vector3(2, 4, 6), "box bounds follow center and size");
        Expect.That(box.ClosestPoint(new Vector3(10, 2, 3)) == new Vector3(2, 2, 3), "box closest point outside");
        Expect.That(box.ClosestPoint(new Vector3(1, 2, 3)) == new Vector3(1, 2, 3), "box closest point inside is the point");
        Expect.That(box.ClosestPointOnBounds(new Vector3(1, 20, 3)) == new Vector3(1, 4, 3), "closest point on bounds");
        var sphere = new SphereCollider { center = new Vector3(0, 1, 0) };
        Expect.That(sphere.radius == 0.5f && sphere.bounds.size == Vector3.one, "sphere defaults");
        Expect.That(sphere.ClosestPoint(new Vector3(0, 11, 0)) == new Vector3(0, 1.5f, 0), "sphere closest point");
        var capsule = new CapsuleCollider();
        Expect.That(capsule.direction == 1 && capsule.height == 2f && capsule.radius == 0.5f, "capsule defaults");
        Expect.That(capsule.bounds.size == new Vector3(1, 2, 1), "capsule bounds along Y");
        capsule.direction = 2;
        Expect.That(capsule.bounds.size == new Vector3(1, 1, 2), "capsule bounds along Z");
        Expect.That(capsule.ClosestPoint(new Vector3(0, 0, 5)) == new Vector3(0, 0, 1), "capsule cap point");
        Expect.Throws<ArgumentOutOfRangeException>(() => capsule.direction = 3, "capsule direction above 2");
        Expect.Throws<ArgumentOutOfRangeException>(() => capsule.direction = -1, "capsule direction below 0");
        var mesh = new MeshCollider();
        Expect.That(mesh.sharedMesh == null && !mesh.convex && (int)mesh.cookingOptions == 30, "mesh collider defaults");
        Expect.Throws<NotSupportedException>(() => mesh.ClosestPoint(Vector3.zero), "mesh collider closest point has no mesh data");
        Expect.Throws<NotSupportedException>(() => box.Raycast(new Ray(Vector3.zero, Vector3.forward), out _, 10f), "Collider.Raycast");
        Expect.That(box.attachedRigidbody == null && box.attachedArticulationBody == null, "a free collider has no body");
    }

    private static void ColliderMaterials()
    {
        var box = new BoxCollider();
        Expect.That(box.sharedMaterial == null, "no shared material by default");
        var shared = new PhysicsMaterial("slick") { dynamicFriction = 0.1f, bounciness = 0.9f };
        box.sharedMaterial = shared;
        Expect.That(ReferenceEquals(box.sharedMaterial, shared), "shared material identity");
        var instance = box.material;
        Expect.That(!ReferenceEquals(instance, shared) && instance.dynamicFriction == 0.1f && instance.bounciness == 0.9f, "material is a copy of the shared one");
        Expect.That(ReferenceEquals(box.material, instance), "the instance is made once");
        instance.dynamicFriction = 0.5f;
        Expect.That(shared.dynamicFriction == 0.1f, "editing the instance leaves the shared material");
        box.sharedMaterial = null;
        Expect.That(!ReferenceEquals(box.material, instance), "assigning a shared material drops the instance");
        var defaults = new PhysicsMaterial();
        Expect.That(defaults.dynamicFriction == 0.6f && defaults.staticFriction == 0.6f && defaults.bounciness == 0f, "material defaults");
        Expect.That(defaults.frictionCombine == PhysicsMaterialCombine.Average && defaults.bounceCombine == PhysicsMaterialCombine.Average, "combine defaults");
    }

    private static void RigidbodyState()
    {
        var body = new Rigidbody();
        Expect.That(body.mass == 1f && body.useGravity && !body.isKinematic && body.detectCollisions, "rigidbody defaults");
        Expect.That(body.angularDamping == 0.05f && body.linearDamping == 0f && body.maxAngularVelocity == 7f, "damping defaults");
        Expect.That(body.solverIterations == 6 && body.solverVelocityIterations == 1, "solver defaults");
        Expect.That(body.constraints == RigidbodyConstraints.None && !body.freezeRotation, "no constraints by default");
        body.freezeRotation = true;
        Expect.That(body.constraints == RigidbodyConstraints.FreezeRotation, "freezeRotation sets the rotation axes");
        body.constraints |= RigidbodyConstraints.FreezePositionY;
        body.freezeRotation = false;
        Expect.That(body.constraints == RigidbodyConstraints.FreezePositionY, "clearing freezeRotation keeps position freezes");
        body.constraints = RigidbodyConstraints.FreezeRotationX;
        Expect.That(!body.freezeRotation, "one frozen rotation axis is not freezeRotation");
        body.linearVelocity = new Vector3(1, 0, 0);
        body.angularVelocity = new Vector3(0, 0, 2);
        Expect.That(body.velocity == body.linearVelocity, "velocity is linearVelocity");
        body.linearDamping = 3f;
        Expect.That(body.drag == 3f, "drag is linearDamping");
        Expect.That(body.GetPointVelocity(new Vector3(0, 1, 0)) == new Vector3(-1, 0, 0), "point velocity adds the spin");
        body.MovePosition(new Vector3(5, 0, 0));
        body.centerOfMass = new Vector3(1, 0, 0);
        Expect.That(body.worldCenterOfMass == new Vector3(6, 0, 0), "world center of mass");
        body.Sleep();
        Expect.That(body.IsSleeping(), "Sleep");
        body.WakeUp();
        Expect.That(!body.IsSleeping(), "WakeUp");
        body.AddForce(Vector3.up, ForceMode.Impulse);
        Expect.That(body.GetAccumulatedForce() == Vector3.zero, "forces have no effect without a simulation");
        Expect.That(!body.SweepTest(Vector3.forward, out var hit) && hit.distance == 0f, "SweepTest finds nothing");
        Expect.That(body.SweepTestAll(Vector3.forward).Length == 0, "SweepTestAll finds nothing");
    }

    private static void CollisionData()
    {
        var empty = new Collision();
        Expect.That(empty.collider == null && empty.gameObject == null && empty.rigidbody == null && empty.body == null, "default collision has no objects");
        Expect.That(empty.contactCount == 0 && empty.contacts.Length == 0 && empty.impulse == Vector3.zero, "default collision has no contacts");
        Expect.Throws<ArgumentOutOfRangeException>(() => empty.GetContact(0), "GetContact on an empty collision");
        Expect.Throws<ArgumentNullException>(() => empty.GetContacts((List<ContactPoint>)null!), "GetContacts(null list)");
        Expect.Throws<ArgumentNullException>(() => empty.GetContacts((ContactPoint[])null!), "GetContacts(null array)");
        var list = new List<ContactPoint> { default };
        Expect.That(empty.GetContacts(list) == 0 && list.Count == 0, "GetContacts clears the list");
        var contact = (ContactPoint)typeof(ContactPoint).GetConstructors(BindingFlags.NonPublic | BindingFlags.Instance)[0]
            .Invoke([new Vector3(1, 2, 3), Vector3.up, Vector3.zero, 0.25f, (default(EntityId), default(EntityId))]);
        var collision = (Collision)typeof(Collision).GetConstructors(BindingFlags.NonPublic | BindingFlags.Instance)[0]
            .Invoke([null, new BoxCollider(), new[] { contact }, new Vector3(0, 4, 0), new Vector3(1, 0, 0)]);
        Expect.That(collision.contactCount == 1 && collision.GetContact(0).point == new Vector3(1, 2, 3) && collision.GetContact(0).separation == 0.25f, "contact data");
        Expect.That(collision.impulse == new Vector3(0, 4, 0) && collision.relativeVelocity == new Vector3(1, 0, 0), "collision impulse and velocity");
        var copy = new ContactPoint[3];
        Expect.That(collision.GetContacts(copy) == 1 && copy[0].normal == Vector3.up, "GetContacts into an array");
        collision.contacts[0] = default;
        Expect.That(collision.GetContact(0).point == new Vector3(1, 2, 3), "contacts returns a copy");
    }
}
