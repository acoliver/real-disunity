using System.Text;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>The calls that record command buffer work in the world's offscreen list, one per kind of record.</summary>
internal static unsafe partial class WorldGraphics
{
    internal static void RecordSetRenderTarget(DisunityTargetRef[] colors, DisunityTargetRef? depth, uint mip, int face, int slice)
    {
        var world = World;
        var withDepth = depth.GetValueOrDefault();
        fixed (DisunityTargetRef* list = colors)
            Check(NativeBindings.disunity_offscreen_set_render_target(world, colors.Length == 0 ? null : list, (uint)colors.Length,
                depth.HasValue ? &withDepth : null, mip, face, slice));
    }

    /// <summary><paramref name="flags"/> is 1 for the color, 2 for the depth and 4 for the stencil.</summary>
    internal static void RecordClear(uint flags, Color color, float depth, uint stencil)
    {
        var value = stackalloc float[] { color.r, color.g, color.b, color.a };
        Check(NativeBindings.disunity_offscreen_clear_render_target(World, flags, value, depth, stencil));
    }

    /// <summary>Records a global set to a temporary render texture, the one global that has no managed holder and so no place in the
    /// world's global state.</summary>
    internal static void RecordGlobalTexture(string name, (uint Kind, ulong Texture) texture)
    {
        var slot = new DisunityTextureSlot { kind = texture.Kind, texture = texture.Texture, scale_x = 1f, scale_y = 1f };
        var bytes = Encoding.UTF8.GetBytes(name);
        fixed (byte* pointer = bytes)
            Check(NativeBindings.disunity_offscreen_set_global_texture(World, pointer, (uint)bytes.Length, &slot));
    }

    internal static void RecordAllocateTemporary(int nameId, DisunityRenderTexture desc) =>
        Check(NativeBindings.disunity_offscreen_allocate_temporary(World, nameId, &desc));

    internal static void RecordReleaseTemporary(int nameId) => Check(NativeBindings.disunity_offscreen_release_temporary(World, nameId));

    /// <summary>Records the camera's setup: its view and projection, column by column as the world holds them, and its target,
    /// with the buffers the camera had when the script made the call, if any.</summary>
    internal static void RecordSetupCamera(ulong camera, Matrix4x4 view, Matrix4x4 projection, CommandTarget target, CameraBuffers? buffers)
    {
        var matrices = stackalloc float[32];
        for (var column = 0; column < 4; column++)
            for (var row = 0; row < 4; row++)
            {
                matrices[column * 4 + row] = view[row, column];
                matrices[16 + column * 4 + row] = projection[row, column];
            }
        var native = target.ToNative();
        var colors = buffers?.Colors ?? [];
        fixed (ulong* list = colors)
            Check(NativeBindings.disunity_offscreen_setup_camera(World, camera, matrices, &native,
                colors.Length == 0 ? null : list, (uint)colors.Length, buffers?.Depth ?? 0));
    }

    /// <summary>Records a view and projection change, each matrix column by column as the world holds it.</summary>
    internal static void RecordSetViewProjection(Matrix4x4 view, Matrix4x4 projection)
    {
        var matrices = stackalloc float[32];
        for (var column = 0; column < 4; column++)
            for (var row = 0; row < 4; row++)
            {
                matrices[column * 4 + row] = view[row, column];
                matrices[16 + column * 4 + row] = projection[row, column];
            }
        Check(NativeBindings.disunity_offscreen_set_view_projection(World, matrices, matrices + 16));
    }

    /// <summary>Records the rectangle in pixels of the bound targets that the draws after it cover.</summary>
    internal static void RecordSetViewport(Rect pixelRect) =>
        Check(NativeBindings.disunity_offscreen_set_viewport(World, pixelRect.x, pixelRect.y, pixelRect.width, pixelRect.height));

    /// <summary>Records the rectangle in pixels of the bound targets that clips the draws after it.</summary>
    internal static void RecordEnableScissor(Rect pixelRect) =>
        Check(NativeBindings.disunity_offscreen_enable_scissor(World, pixelRect.x, pixelRect.y, pixelRect.width, pixelRect.height));

    /// <summary>Records the end of the clip a scissor rectangle set.</summary>
    internal static void RecordDisableScissor() => Check(NativeBindings.disunity_offscreen_disable_scissor(World));

    /// <summary>Records the constant and slope-scaled depth bias of the draws after it.</summary>
    internal static void RecordDepthBias(float bias, float slopeBias) =>
        Check(NativeBindings.disunity_offscreen_set_depth_bias(World, bias, slopeBias));

    /// <summary>Writes <paramref name="block"/> to the world's draw scope, which the next mesh draw takes as its own property
    /// block. A value the world refuses (a property name longer than it accepts) throws and leaves the scope empty, so no part
    /// of the block reaches a later draw.</summary>
    internal static void StageDrawBlock(PropertyStore block)
    {
        var next = new Link(World, 0);
        ClearProperties(ScopeDraw, next);
        try { SendAll(ScopeDraw, next, block); }
        catch
        {
            ClearProperties(ScopeDraw, next);
            throw;
        }
    }

    /// <summary>Records a request to fill mip levels 1 and up of a render texture from its level 0.</summary>
    internal static void RecordGenerateMips(ulong texture) => Check(NativeBindings.disunity_offscreen_generate_mips(World, texture));

    /// <summary>Records a draw of a stored mesh (its renderer identity) with the matrix column by column, as the world holds it, and
    /// <paramref name="instances"/> instances of the sub-mesh; none is a draw of nothing that keeps its place.</summary>
    internal static void RecordDrawMesh(ulong mesh, Matrix4x4 matrix, ulong material, (int Submesh, int Pass) draw, uint instances)
    {
        var columns = stackalloc float[16];
        for (var column = 0; column < 4; column++)
            for (var row = 0; row < 4; row++)
                columns[column * 4 + row] = matrix[row, column];
        Check(NativeBindings.disunity_offscreen_draw_mesh_instanced(World, mesh, columns, material, (uint)draw.Submesh, draw.Pass, instances));
    }

    /// <summary>Records a draw of generated vertices with the matrix column by column, as the world holds it. Topology 0 is a
    /// triangle list and 1 a line list.</summary>
    internal static void RecordDrawProcedural(Matrix4x4 matrix, ulong material, int pass, uint topology, (int Vertices, int Instances) counts)
    {
        var columns = stackalloc float[16];
        for (var column = 0; column < 4; column++)
            for (var row = 0; row < 4; row++)
                columns[column * 4 + row] = matrix[row, column];
        Check(NativeBindings.disunity_offscreen_draw_procedural(World, columns, material, pass, topology, counts.Vertices, counts.Instances));
    }

    private static long _lastHandle;

    /// <summary>The next id for a culling result or a renderer list. Ids are never reused, so a handle from an earlier frame
    /// names nothing in a later one.</summary>
    internal static ulong NextHandle() => (ulong)Interlocked.Increment(ref _lastHandle);

    /// <summary>Records a culling result: the camera the parameters came from, their layer mask, the planes in use and the options.</summary>
    internal static void RecordCull(ulong id, ScriptableCullingParameters parameters)
    {
        var count = parameters.cullingPlaneCount;
        var floats = new float[count * 4];
        for (var index = 0; index < count; index++)
        {
            var plane = parameters.GetCullingPlane(index);
            (floats[index * 4], floats[index * 4 + 1], floats[index * 4 + 2], floats[index * 4 + 3]) = (plane.normal.x, plane.normal.y, plane.normal.z, plane.distance);
        }
        fixed (float* planes = floats)
            Check(NativeBindings.disunity_offscreen_cull(World, id, parameters.CameraHandle, parameters.cullingMask, count == 0 ? null : planes,
                (uint)count, (uint)parameters.cullingOptions));
    }
}
