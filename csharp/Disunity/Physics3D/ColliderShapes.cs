namespace UnityEngine;

public class BoxCollider : Collider
{
    public Vector3 center { get; set; }
    public Vector3 size { get; set; } = Vector3.one;

    internal override Bounds LocalBounds => new(center, size);

    public override Vector3 ClosestPoint(Vector3 position) => ClosestPointOnBounds(position);
}

public class SphereCollider : Collider
{
    public Vector3 center { get; set; }
    public float radius { get; set; } = 0.5f;

    internal override Bounds LocalBounds => new(center, Vector3.one * (2f * radius));

    public override Vector3 ClosestPoint(Vector3 position) => center + Vector3.ClampMagnitude(position - center, radius);
}

public class CapsuleCollider : Collider
{
    private int _direction = 1;

    public Vector3 center { get; set; }
    public float radius { get; set; } = 0.5f;
    public float height { get; set; } = 2f;

    /// <summary>The axis the capsule's length runs along: 0 for X, 1 for Y, 2 for Z.</summary>
    public int direction
    {
        get => _direction;
        set => _direction = value is >= 0 and <= 2 ? value : throw new ArgumentOutOfRangeException(nameof(value), value, "A capsule direction is 0 (X), 1 (Y) or 2 (Z)");
    }

    internal override Bounds LocalBounds
    {
        get
        {
            var size = Vector3.one * (2f * radius);
            size[_direction] = Mathf.Max(height, 2f * radius);
            return new Bounds(center, size);
        }
    }

    public override Vector3 ClosestPoint(Vector3 position)
    {
        var half = Mathf.Max(height * 0.5f - radius, 0f);
        var axis = Vector3.zero;
        axis[_direction] = 1f;
        var along = Mathf.Clamp(Vector3.Dot(position - center, axis), -half, half);
        var spine = center + axis * along;
        return spine + Vector3.ClampMagnitude(position - spine, radius);
    }
}

public class MeshCollider : Collider
{
    public Mesh? sharedMesh { get; set; }
    public bool convex { get; set; }

    public MeshColliderCookingOptions cookingOptions { get; set; } =
        MeshColliderCookingOptions.CookForFasterSimulation | MeshColliderCookingOptions.EnableMeshCleaning |
        MeshColliderCookingOptions.WeldColocatedVertices | MeshColliderCookingOptions.UseFastMidphase;
}
