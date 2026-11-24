using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;

/// <summary>What the next presented frame draws of a skinned renderer, read through the native interface: the world has no call
/// that reads a skin back, and the frame's mesh object list is where its mesh and placement show.</summary>
internal static class SkinSupport
{
    /// <summary>The mesh identity and the matrix (column-major, in the frame's view basis) of the mesh object the frame lists for
    /// <paramref name="renderer"/>, or null when it lists none.</summary>
    internal static unsafe (ulong Mesh, float[] Matrix)? Placed(SkinnedMeshRenderer renderer)
    {
        var world = DisunityHost.WorldHandle;
        var header = new DisunityPresentation();
        DisunityHost.ThrowIfError(NativeBindings.disunity_presentation_prepare(world, 80, 40, 18, &header));
        for (uint index = 0; index < header.mesh_objects; index++)
        {
            var record = new DisunityMeshObject();
            var matrix = new DisunityMatrix();
            DisunityHost.ThrowIfError(NativeBindings.disunity_presentation_mesh_object(world, index, &record, &matrix, null, 0));
            if (record.@object != renderer.gameObject.NativeHandle) continue;
            var values = new float[16];
            var source = (float*)&matrix;
            for (var i = 0; i < 16; i++) values[i] = source[i];
            return (record.mesh, values);
        }
        return null;
    }

    /// <summary>Whether the matrix puts the mesh at <paramref name="x"/>, <paramref name="y"/>, <paramref name="z"/> in the frame's view
    /// basis, which turns a world position (x, y, z) into (x, -z, y).</summary>
    internal static bool PlacedAtWorld(float[] matrix, float x, float y, float z) =>
        Math.Abs(matrix[12] - x) < 1e-5f && Math.Abs(matrix[13] + z) < 1e-5f && Math.Abs(matrix[14] - y) < 1e-5f;
}
