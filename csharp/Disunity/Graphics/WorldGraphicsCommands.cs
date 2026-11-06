using System.Text;
using Disunity.Native;

namespace UnityEngine;

/// <summary>The calls that record command buffer work in the world's offscreen list, one per kind of record.</summary>
internal static unsafe partial class WorldGraphics
{
    private const uint GlobalFloat = 0, GlobalInt = 1, GlobalVector = 2, GlobalMatrix = 3, GlobalTexture = 4;

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

    internal static void RecordGlobalFloat(string name, float value)
    {
        var floats = stackalloc float[] { value };
        RecordGlobal(GlobalFloat, name, floats, 0, null);
    }

    internal static void RecordGlobalInt(string name, int value) => RecordGlobal(GlobalInt, name, null, value, null);

    internal static void RecordGlobalVector(string name, Vector4 value)
    {
        var floats = stackalloc float[] { value.x, value.y, value.z, value.w };
        RecordGlobal(GlobalVector, name, floats, 0, null);
    }

    /// <summary>Sends the matrix column by column, as the world holds it.</summary>
    internal static void RecordGlobalMatrix(string name, Matrix4x4 m)
    {
        var floats = stackalloc float[]
        {
            m.m00, m.m10, m.m20, m.m30, m.m01, m.m11, m.m21, m.m31, m.m02, m.m12, m.m22, m.m32, m.m03, m.m13, m.m23, m.m33,
        };
        RecordGlobal(GlobalMatrix, name, floats, 0, null);
    }

    internal static void RecordGlobalTexture(string name, (uint Kind, ulong Texture) texture)
    {
        var slot = new DisunityTextureSlot { kind = texture.Kind, texture = texture.Texture, scale_x = 1f, scale_y = 1f };
        RecordGlobal(GlobalTexture, name, null, 0, &slot);
    }

    private static void RecordGlobal(uint kind, string name, float* floats, int intValue, DisunityTextureSlot* slot)
    {
        var bytes = Encoding.UTF8.GetBytes(name);
        fixed (byte* pointer = bytes)
            Check(NativeBindings.disunity_offscreen_set_global(World, kind, pointer, (uint)bytes.Length, floats, intValue, slot));
    }

    internal static void RecordGlobalKeyword(string name, bool enabled)
    {
        var bytes = Encoding.UTF8.GetBytes(name);
        fixed (byte* pointer = bytes)
            Check(NativeBindings.disunity_offscreen_set_global_keyword(World, pointer, (uint)bytes.Length, enabled ? 1u : 0u));
    }

    internal static void RecordAllocateTemporary(int nameId, DisunityRenderTexture desc) =>
        Check(NativeBindings.disunity_offscreen_allocate_temporary(World, nameId, &desc));

    internal static void RecordReleaseTemporary(int nameId) => Check(NativeBindings.disunity_offscreen_release_temporary(World, nameId));
}
