namespace UnityEngine.LowLevelPhysics;

public enum GeometryType { Sphere = 0, Capsule = 2, Box = 3, ConvexMesh = 4, TriangleMesh = 5, Terrain = 6, Invalid = -1 }

/// <summary>A shape description. The engine defines no geometry types, so only callers' own
/// structs implement it.</summary>
public interface IGeometry
{
    GeometryType GeometryType { get; }
}

/// <summary>Holds one geometry value with its type; a default holder is invalid.</summary>
public readonly struct GeometryHolder
{
    private readonly object? m_Geometry;
    private readonly GeometryType m_Type;

    private GeometryHolder(object geometry, GeometryType type)
    {
        m_Geometry = geometry;
        m_Type = type;
    }

    public GeometryType Type => m_Geometry is null ? GeometryType.Invalid : m_Type;

    public static GeometryHolder Create<T>(T geometry) where T : struct, IGeometry => new(geometry, geometry.GeometryType);

    public T As<T>() where T : struct, IGeometry =>
        m_Geometry is T value ? value : throw new InvalidOperationException($"The geometry holder does not hold a {typeof(T).Name}");
}
