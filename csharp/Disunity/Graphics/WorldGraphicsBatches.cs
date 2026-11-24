using System.Text;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

internal static unsafe partial class WorldGraphics
{
    /// <summary>Records a draw of a batch: the mesh and material, the window of the batch's buffer (the whole buffer when the batch
    /// named no window), the metadata words by property name, the visible instance indexes and the pass tags of the list drawing it.
    /// A draw whose mesh, material, sub-mesh or buffer is gone, or that the world refuses, is left out and noted once.</summary>
    internal static void RecordDrawBatch(BatchDraw draw, string[] tags)
    {
        var (mesh, material, batch) = (draw.Mesh, draw.Material, draw.Batch);
        var buffer = GraphicsBuffer.Find(batch.Buffer);
        if (!mesh || !material || buffer is null || draw.Submesh >= mesh.subMeshCount)
        {
            UndrawnCalls.NoteBatchFailure("a batch draw names a mesh, material, sub-mesh or buffer that no longer exists");
            return;
        }
        var names = batch.Metadata.Select(row => Shader.PropertyName(row.NameID) ?? "").ToArray();
        var words = batch.Metadata.Select(row => row.Value).ToArray();
        var namesBytes = Encoding.UTF8.GetBytes(string.Join('\0', names));
        var tagsBytes = Encoding.UTF8.GetBytes(string.Join('\0', tags));
        var record = new DisunityDrawBatch
        {
            mesh = mesh.RendererIdentity,
            material = material.WorldLink.Id,
            buffer = buffer.WorldLink.Id,
            offset = batch.BufferOffset,
            size = batch.WindowSize == 0 ? (ulong)buffer.ByteSize - batch.BufferOffset : batch.WindowSize,
            submesh = (uint)draw.Submesh,
            pass = -1,
            metadata_count = (uint)words.Length,
            names_len = (uint)namesBytes.Length,
            visible_count = (uint)draw.Visible.Length,
            tags_len = (uint)tagsBytes.Length,
        };
        try
        {
            fixed (byte* namesPointer = namesBytes)
            fixed (byte* tagsPointer = tagsBytes)
            fixed (uint* wordsPointer = words)
            fixed (uint* visiblePointer = draw.Visible)
                Check(NativeBindings.disunity_offscreen_draw_mesh_batch(World, &record, namesPointer, wordsPointer, visiblePointer, tagsPointer));
        }
        catch (DisunityException refused)
        {
            UndrawnCalls.NoteBatchFailure(refused.Message);
        }
    }
}
