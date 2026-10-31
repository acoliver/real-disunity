namespace UnityEngine;

public partial class Mesh
{
    /// <summary>A copy of the whole mesh, named like a clone: the name gets "(Clone)" appended.</summary>
    internal override Object CloneAsset()
    {
        var copy = new Mesh();
        copy.CopyDataFrom(this);
        return Named(copy);
    }

    /// <summary>Replaces this mesh's channels, sub-meshes, bounds and index format with copies of
    /// <paramref name="source"/>'s. It ignores <see cref="isReadable"/> on both meshes, as the renderer's view does,
    /// and leaves this mesh's readability as it was.</summary>
    internal void CopyDataFrom(Mesh source)
    {
        var data = source.Snapshot();
        _positions = data.Positions;
        _normals = data.Normals;
        _tangents = data.Tangents;
        _colors = data.Colors;
        for (var channel = 0; channel < UvChannelCount; channel++)
        {
            _uvs[channel] = data.Uvs[channel];
            _uvDimensions[channel] = data.UvDimensions[channel];
        }
        _subMeshes.Clear();
        foreach (var subMesh in data.SubMeshes)
            _subMeshes.Add(new SubMesh { Indices = subMesh.Indices, Topology = subMesh.Topology, BaseVertex = subMesh.BaseVertex });
        _bounds = data.Bounds;
        _indexFormat = data.IndexFormat;
    }
}
