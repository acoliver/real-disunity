using Disunity.Native;

namespace UnityEngine;

/// <summary>The calls that read pixels of a render texture back: one records the request in the offscreen list, the host's flush
/// answers it, and the other takes the answer.</summary>
internal static unsafe partial class WorldGraphics
{
    /// <summary>Records a read of a rectangle of mip 0 of a render texture and returns the id of the request.</summary>
    internal static ulong RecordReadPixels(ulong texture, uint x, uint y, uint width, uint height)
    {
        ulong request;
        Check(NativeBindings.disunity_render_texture_read_pixels(World, texture, x, y, width, height, &request));
        return request;
    }

    /// <summary>Takes the texels the host wrote for a request, four floats each and the bottom row first, or null when it wrote
    /// nothing.</summary>
    internal static float[]? TakePixels(ulong request, int floatCount)
    {
        var data = new float[floatCount];
        uint written;
        fixed (float* pointer = data)
            Check(NativeBindings.disunity_render_texture_pixels_take(World, request, pointer, (uint)floatCount, &written));
        return written == 0 ? null : data;
    }
}
