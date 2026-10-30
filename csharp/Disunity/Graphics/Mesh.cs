namespace UnityEngine;

/// <summary>
/// A mesh asset. It holds the CPU-side vertex data a game builds at run time: positions, normals, tangents,
/// colors, eight uv channels, and sub-meshes of indices with a topology and a base vertex. Nothing draws a mesh
/// yet; the data is kept for the renderer, which reads it through <see cref="Snapshot"/>.
/// A channel is either empty or as long as <see cref="vertexCount"/>. Getters return copies, and the
/// <c>Get...(List)</c> forms clear the list and then fill it. A null array or list sets an empty channel.
/// <see cref="UploadMeshData"/> with true makes the mesh unreadable for the game; <see cref="bounds"/>,
/// <see cref="vertexCount"/>, <see cref="subMeshCount"/> and the sub-mesh queries keep working.
/// </summary>
public partial class Mesh : Object
{
    private const int UvChannelCount = 8;

    private Vector3[] _positions = [];
    private Vector3[] _normals = [];
    private Vector4[] _tangents = [];
    private Color[] _colors = [];
    private readonly Vector4[][] _uvs = NewUvChannels();
    private readonly int[] _uvDimensions = new int[UvChannelCount];
    private Bounds _bounds;
    private bool _readable = true;

    public Mesh()
    {
        ResetSubMeshes();
    }

    public int vertexCount => _positions.Length;

    public bool isReadable => _readable;

    /// <summary>The box the renderer culls with. It changes when set, by <see cref="RecalculateBounds"/>, and when
    /// <see cref="triangles"/> or a <c>SetTriangles</c> or <c>SetIndices</c> call asks for it; setting
    /// <see cref="vertices"/> leaves it as it was.</summary>
    public Bounds bounds
    {
        get => _bounds;
        set => _bounds = value;
    }

    public void MarkDynamic()
    {
    }

    /// <summary>Marks the mesh unreadable when <paramref name="markNoLongerReadable"/> is true. The engine keeps its
    /// copy of the data for the renderer.</summary>
    public void UploadMeshData(bool markNoLongerReadable)
    {
        if (markNoLongerReadable) _readable = false;
    }

    /// <summary>The engine's choice: the data stays as it is, because reordering has no effect without a GPU upload.</summary>
    public void Optimize()
    {
    }

    public void OptimizeIndexBuffers()
    {
    }

    public void OptimizeReorderVertexBuffer()
    {
    }

    /// <summary>Empties every channel and sub-mesh. The mesh then has one sub-mesh without indices and zero bounds.</summary>
    public void Clear() => Clear(true);

    public void Clear(bool keepVertexLayout)
    {
        RequireReadable();
        _positions = [];
        _normals = [];
        _tangents = [];
        _colors = [];
        for (var channel = 0; channel < UvChannelCount; channel++)
        {
            _uvs[channel] = [];
            _uvDimensions[channel] = 0;
        }
        ResetSubMeshes();
        _bounds = default;
    }

    private void RequireReadable()
    {
        if (!_readable) throw new InvalidOperationException("mesh is not readable");
    }

    private static Vector4[][] NewUvChannels()
    {
        var channels = new Vector4[UvChannelCount][];
        Array.Fill(channels, Array.Empty<Vector4>());
        return channels;
    }
}
