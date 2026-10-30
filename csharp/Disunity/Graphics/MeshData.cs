using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>A copy of a mesh's CPU-side data for the renderer. It ignores <see cref="Mesh.isReadable"/>, which limits only the game.</summary>
internal sealed class MeshData
{
    public Vector3[] Positions { get; init; } = [];
    public Vector3[] Normals { get; init; } = [];
    public Vector4[] Tangents { get; init; } = [];
    public Color[] Colors { get; init; } = [];

    /// <summary>The eight uv channels as four-component values; the dimension of each is in <see cref="UvDimensions"/>.</summary>
    public Vector4[][] Uvs { get; init; } = [];

    /// <summary>The component count (2, 3 or 4) of the last set for each uv channel, or 0 for an empty channel.</summary>
    public int[] UvDimensions { get; init; } = [];

    public MeshSubMeshData[] SubMeshes { get; init; } = [];
    public Bounds Bounds { get; init; }
    public IndexFormat IndexFormat { get; init; }
}

internal readonly record struct MeshSubMeshData(int[] Indices, MeshTopology Topology, int BaseVertex);
