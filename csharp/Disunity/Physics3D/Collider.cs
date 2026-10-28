using UnityEngine.LowLevelPhysics;

namespace UnityEngine;

/// <summary>
/// Base of the 3D colliders. A collider cannot be attached to a GameObject (see
/// <see cref="PhysicsComponents"/>), so an instance is never part of a world: its settings are plain
/// stored state and its geometry is measured in its own local space.
/// </summary>
public class Collider : Component
{
    private PhysicsMaterial? _shared;
    private PhysicsMaterial? _instance;

    public bool enabled { get; set; } = true;
    public Rigidbody? attachedRigidbody => null;
    public ArticulationBody? attachedArticulationBody => null;
    public bool isTrigger { get; set; }
    public float contactOffset { get; set; } = Physics.defaultContactOffset;
    public bool hasModifiableContacts { get; set; }
    public bool providesContacts { get; set; }
    public int layerOverridePriority { get; set; }
    public LayerMask excludeLayers { get; set; }
    public LayerMask includeLayers { get; set; }

    public PhysicsMaterial? sharedMaterial
    {
        get => _shared;
        set
        {
            _shared = value;
            _instance = null;
        }
    }

    /// <summary>A private copy of the shared material, made on first use as Unity does.</summary>
    public PhysicsMaterial material
    {
        get => _instance ??= CopyOf(_shared);
        set => _instance = value;
    }

    public Bounds bounds => LocalBounds;

    public GeometryHolder GeometryHolder =>
        throw new NotSupportedException("Collider.GeometryHolder is not supported yet: the engine defines no geometry types");

    public T GetGeometry<T>() where T : struct, IGeometry =>
        throw new NotSupportedException("Collider.GetGeometry is not supported yet: the engine defines no geometry types");

    public bool Raycast(Ray ray, out RaycastHit hitInfo, float maxDistance) =>
        throw new NotSupportedException("Collider.Raycast is not supported yet");

    public Vector3 ClosestPointOnBounds(Vector3 position)
    {
        var box = LocalBounds;
        return Vector3.Max(box.min, Vector3.Min(box.max, position));
    }

    public virtual Vector3 ClosestPoint(Vector3 position) =>
        throw new NotSupportedException($"{GetType().Name}.ClosestPoint is not supported yet");

    /// <summary>The collider's axis-aligned box in its own space, before any transform.</summary>
    internal virtual Bounds LocalBounds => default;

    private static PhysicsMaterial CopyOf(PhysicsMaterial? source) => new((source?.name ?? "Default") + " (Instance)")
    {
        bounciness = source?.bounciness ?? 0f,
        dynamicFriction = source?.dynamicFriction ?? 0.6f,
        staticFriction = source?.staticFriction ?? 0.6f,
        frictionCombine = source?.frictionCombine ?? PhysicsMaterialCombine.Average,
        bounceCombine = source?.bounceCombine ?? PhysicsMaterialCombine.Average,
    };
}
