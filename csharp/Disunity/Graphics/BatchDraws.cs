using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// One draw command a culling callback wrote, resolved against the group that holds its mesh, material and batch: a run of
/// visible instance indexes of one batch drawn with one mesh and material, and the filter settings of the draw range it was in.
/// </summary>
internal sealed record BatchDraw(Mesh Mesh, Material Material, int Submesh, BatchRendererGroup.Batch Batch, uint[] Visible, BatchFilterSettings Filter);

/// <summary>
/// What the live batch renderer groups draw for one view. <see cref="Run"/> asks each group's culling callback, reads the draw
/// commands it wrote and frees the arrays the callback allocated. A list made from the view takes the draws it accepts (see
/// <see cref="BatchListDraws"/>); nothing is asked again per list.
/// </summary>
internal sealed class BatchCull
{
    private BatchCull(IReadOnlyList<BatchDraw> draws, uint layerMask) => (Draws, LayerMask) = (draws, layerMask);

    internal IReadOnlyList<BatchDraw> Draws { get; }

    /// <summary>The culling mask the view was culled with.</summary>
    internal uint LayerMask { get; }

    /// <summary>Asks every live group's callback to cull for the view, in the order the groups were made.</summary>
    internal static BatchCull Run(BatchCullingViewType view, IReadOnlyList<Plane> planes, uint layerMask)
    {
        var groups = BatchRendererGroup.LiveGroups();
        if (groups.Length == 0) return new BatchCull([], layerMask);
        var array = new NativeArray<Plane>(planes.Count, Allocator.Temp);
        for (var index = 0; index < planes.Count; index++) array[index] = planes[index];
        try
        {
            var context = new BatchCullingContext { viewType = view, cullingPlanes = array, cullingLayerMask = layerMask };
            return new BatchCull(groups.SelectMany(group => group.Cull(context)).ToArray(), layerMask);
        }
        finally
        {
            array.Dispose();
        }
    }
}

/// <summary>Reads the draw commands a culling callback wrote: each command of each draw range, resolved through the group's
/// registrations to the mesh, material and batch it names, with the visible instance indexes it covers.</summary>
internal static unsafe class BatchOutputReader
{
    internal static List<BatchDraw> Read(BatchRendererGroup group, in BatchCullingOutputDrawCommands output)
    {
        var draws = new List<BatchDraw>();
        for (var index = 0; index < output.drawRangeCount; index++)
        {
            var range = output.drawRanges[index];
            var end = Math.Min((long)range.drawCommandsBegin + range.drawCommandsCount, output.drawCommandCount);
            for (var command = (long)range.drawCommandsBegin; command < end; command++)
            {
                if (Resolve(group, output, output.drawCommands[command], range.filterSettings) is { } draw) draws.Add(draw);
            }
        }
        return draws;
    }

    /// <summary>The draw a command names, or null for a command that draws nothing or names what the group does not hold or
    /// visible instances the callback did not write; the last two are noted once.</summary>
    private static BatchDraw? Resolve(BatchRendererGroup group, in BatchCullingOutputDrawCommands output, BatchDrawCommand command, BatchFilterSettings filter)
    {
        if (command.visibleCount == 0) return null;
        var (mesh, material, batch) = (group.GetRegisteredMesh(command.meshID), group.GetRegisteredMaterial(command.materialID), group.GetBatch(command.batchID));
        if (mesh is null || material is null || batch is null)
        {
            UndrawnCalls.NoteBatchFailure("a draw command names a mesh, material or batch the group does not hold");
            return null;
        }
        if ((long)command.visibleOffset + command.visibleCount > output.visibleInstanceCount)
        {
            UndrawnCalls.NoteBatchFailure("a draw command names visible instances past the ones the callback wrote");
            return null;
        }
        var visible = new uint[command.visibleCount];
        for (var index = 0; index < visible.Length; index++) visible[index] = (uint)output.visibleInstances[command.visibleOffset + index];
        return new BatchDraw(mesh, material, command.submeshIndex, batch, visible, filter);
    }
}
