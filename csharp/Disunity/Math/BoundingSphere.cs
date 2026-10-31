namespace UnityEngine;

/// <summary>A sphere given by a center and a radius. It is plain data; equality compares both fields exactly.</summary>
public struct BoundingSphere : IEquatable<BoundingSphere>
{
    public Vector3 position;
    public float radius;

    public BoundingSphere(Vector3 pos, float rad)
    {
        position = pos;
        radius = rad;
    }

    /// <summary>The position is x, y, z and the radius is w.</summary>
    public BoundingSphere(Vector4 packedSphere) : this(new Vector3(packedSphere.x, packedSphere.y, packedSphere.z), packedSphere.w) { }

    public readonly bool Equals(BoundingSphere other) => position.Equals(other.position) && radius.Equals(other.radius);
    public override readonly bool Equals(object? obj) => obj is BoundingSphere other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(position, radius);
    public static bool operator ==(BoundingSphere left, BoundingSphere right) => left.Equals(right);
    public static bool operator !=(BoundingSphere left, BoundingSphere right) => !left.Equals(right);
    public override readonly string ToString() => $"BoundingSphere({position}, {radius})";
}
