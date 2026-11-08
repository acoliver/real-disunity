using System.Runtime.InteropServices;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Texture2D.ReadPixels copies a rectangle of RenderTexture.active into the texture: the managed side records a request,
/// flushes, takes what the host wrote and puts it in with the SetPixels path. The test host answers with a color that names the
/// render texture texel it came from, so a misplaced row or column shows.</summary>
internal static unsafe class ReadPixelsTests
{
    private enum Mode { Answer, Silent, Fail }

    private static Mode _mode;
    private static readonly List<(ulong Texture, uint X, uint Y, uint Width, uint Height)> Requests = [];

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            RowsAndColumnsLandWhereTheyBelong();
            Console.WriteLine("PASS: Texture2D.ReadPixels puts the render texture rectangle at the destination, bottom row first, through the SetPixels path");
            RectIsFlooredAndRounded();
            Console.WriteLine("PASS: Texture2D.ReadPixels floors the rectangle position and rounds its size to whole texels");
            DestinationIsClipped();
            Console.WriteLine("PASS: Texture2D.ReadPixels writes the part of the rectangle inside the texture and requests only that part");
            WithoutAnAnswerTheTextureIsUnchanged();
            Console.WriteLine("PASS: Texture2D.ReadPixels keeps the texels when the host wrote nothing or the flush failed, and does not throw");
            TheScreenIsNotSupported();
            Console.WriteLine("PASS: Texture2D.ReadPixels from the screen throws NotSupportedException naming the member");
            BadRectsAndTexturesThrow();
            Console.WriteLine("PASS: Texture2D.ReadPixels throws for a rectangle outside the render texture, a negative size and an unreadable texture, and records nothing");
            MipsFollowWhenAsked();
            Console.WriteLine("PASS: Texture2D.ReadPixels regenerates the smaller mip levels when asked (by default), and the GPU copy waits for Apply");
        }
        finally
        {
            Unregister();
            RenderTexture.active = null;
            DisunityHost.UnloadWorld();
        }
    }

    private static void Register() =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_gpu_flush_register(DisunityHost.WorldHandle, &Flush, 0));

    private static void Unregister() =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_gpu_flush_register(DisunityHost.WorldHandle, null, 0));

    /// <summary>The red channel of a texel is its column over 100 and the green channel its row over 100.</summary>
    private static Color Expected(int column, int row) => new(column / 100f, row / 100f, 0.5f, 1f);

    [UnmanagedCallersOnly(CallConvs = [typeof(System.Runtime.CompilerServices.CallConvCdecl)])]
    private static DisunityResult Flush(ulong world, ulong user, byte* message, uint capacity, uint* length)
    {
        if (_mode == Mode.Fail)
        {
            message[0] = (byte)'x';
            *length = 1;
            return new DisunityResult { code = 3 };
        }
        foreach (var record in ComputeTestSupport.TakeWork(world))
        {
            if (record.kind != 12) continue;
            var bytes = new byte[64];
            uint size;
            fixed (byte* pointer = bytes)
                DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_detail(world, record.bindings, pointer, 64, &size));
            var request = BitConverter.ToUInt64(bytes, 1);
            var texture = BitConverter.ToUInt64(bytes, 9);
            var (x, y, width, height) = (BitConverter.ToUInt32(bytes, 17), BitConverter.ToUInt32(bytes, 21), BitConverter.ToUInt32(bytes, 25), BitConverter.ToUInt32(bytes, 29));
            Requests.Add((texture, x, y, width, height));
            if (_mode == Mode.Silent) continue;
            var floats = new float[width * height * 4];
            for (var row = 0; row < height; row++)
                for (var column = 0; column < width; column++)
                {
                    var color = Expected((int)(x + column), (int)(y + row));
                    var at = (int)(row * width + column) * 4;
                    (floats[at], floats[at + 1], floats[at + 2], floats[at + 3]) = (color.r, color.g, color.b, color.a);
                }
            fixed (float* pointer = floats)
                DisunityHost.ThrowIfError(NativeBindings.disunity_render_texture_pixels_write(world, request, pointer, (uint)floats.Length));
        }
        return default;
    }

    private static Texture2D Filled(int width, int height, bool mips = false)
    {
        var texture = new Texture2D(width, height, TextureFormat.RGBAFloat, mips);
        var colors = new Color[width * height];
        Array.Fill(colors, Color.magenta);
        texture.SetPixels(colors);
        texture.Apply();
        return texture;
    }

    private static void Begin(Mode mode)
    {
        _mode = mode;
        Requests.Clear();
        Register();
    }

    private static void RowsAndColumnsLandWhereTheyBelong()
    {
        Begin(Mode.Answer);
        var source = new RenderTexture(8, 6, 0);
        RenderTexture.active = source;
        var target = Filled(10, 8);
        target.ReadPixels(new Rect(2, 1, 3, 2), 4, 3);
        Ensure(Requests.Count == 1, $"one request, got {Requests.Count}");
        Ensure(Requests[0] == (source.WorldId, 2, 1, 3, 2), $"the request names the active texture and the rectangle: {Requests[0]}");
        for (var y = 0; y < 8; y++)
            for (var x = 0; x < 10; x++)
            {
                var inside = x is >= 4 and < 7 && y is >= 3 and < 5;
                var expected = inside ? Expected(2 + x - 4, 1 + y - 3) : Color.magenta;
                Near(expected, target.GetPixel(x, y), 1e-6f, $"texel ({x}, {y})");
            }
    }

    private static void RectIsFlooredAndRounded()
    {
        Begin(Mode.Answer);
        var source = new RenderTexture(8, 6, 0);
        RenderTexture.active = source;
        var target = Filled(8, 6);
        target.ReadPixels(new Rect(1.9f, 0.2f, 2.4f, 1.6f), 0, 0);
        Ensure(Requests[0] == (source.WorldId, 1, 0, 2, 2), $"the rectangle becomes whole texels: {Requests[0]}");
        target.ReadPixels(new Rect(0, 0, 8, 6), 0, 0, false);
        Ensure(Requests[1] == (source.WorldId, 0, 0, 8, 6), $"a full read: {Requests[1]}");
        Near(Expected(7, 5), target.GetPixel(7, 5), 1e-6f, "the far corner");
    }

    private static void DestinationIsClipped()
    {
        Begin(Mode.Answer);
        var source = new RenderTexture(8, 6, 0);
        RenderTexture.active = source;
        var target = Filled(10, 8);
        target.ReadPixels(new Rect(1, 1, 4, 3), 8, 6);
        Ensure(Requests[0] == (source.WorldId, 1, 1, 2, 2), $"only the part that lands inside is requested: {Requests[0]}");
        Near(Expected(1, 1), target.GetPixel(8, 6), 1e-6f, "the first texel inside");
        Near(Expected(2, 2), target.GetPixel(9, 7), 1e-6f, "the last texel inside");
        Near(Color.magenta, target.GetPixel(7, 6), 1e-6f, "left of the block");
        Requests.Clear();
        target.ReadPixels(new Rect(2, 1, 4, 3), -1, -1);
        Ensure(Requests[0] == (source.WorldId, 3, 2, 3, 2), $"a negative destination drops the first column and row: {Requests[0]}");
        Near(Expected(3, 2), target.GetPixel(0, 0), 1e-6f, "the texel at the origin");
        Requests.Clear();
        target.ReadPixels(new Rect(0, 0, 2, 2), 10, 0);
        target.ReadPixels(new Rect(0, 0, 2, 2), 0, -2);
        Ensure(Requests.Count == 0, "a destination wholly outside requests nothing");
    }

    private static void WithoutAnAnswerTheTextureIsUnchanged()
    {
        var source = new RenderTexture(4, 4, 0);
        RenderTexture.active = source;
        foreach (var mode in new[] { Mode.Silent, Mode.Fail })
        {
            Begin(mode);
            var target = Filled(4, 4);
            using var log = new LogRecorder();
            target.ReadPixels(new Rect(0, 0, 4, 4), 0, 0);
            Near(Color.magenta, target.GetPixel(1, 1), 1e-6f, $"unchanged after a {mode} flush");
        }
        Unregister();
        var plain = Filled(4, 4);
        plain.ReadPixels(new Rect(0, 0, 4, 4), 0, 0);
        Near(Color.magenta, plain.GetPixel(3, 3), 1e-6f, "unchanged without a flush handler");
    }

    private static void TheScreenIsNotSupported()
    {
        Begin(Mode.Answer);
        RenderTexture.active = null;
        var target = Filled(4, 4);
        Throws<NotSupportedException>(() => target.ReadPixels(new Rect(0, 0, 4, 4), 0, 0), "the screen", "Texture2D.ReadPixels from the screen");
        Throws<NotSupportedException>(() => target.ReadPixels(new Rect(0, 0, 4, 4), 0, 0, true), "the screen with mips", "Texture2D.ReadPixels from the screen");
        Ensure(Requests.Count == 0, "nothing was requested");
    }

    private static void BadRectsAndTexturesThrow()
    {
        Begin(Mode.Answer);
        var source = new RenderTexture(8, 6, 0);
        RenderTexture.active = source;
        var target = Filled(8, 6);
        ComputeTestSupport.TakeWork(DisunityHost.WorldHandle);
        var outside = new[]
        {
            new Rect(-1, 0, 2, 2), new Rect(0, -1, 2, 2), new Rect(7, 0, 2, 1), new Rect(0, 5, 1, 2), new Rect(0, 0, 9, 1),
            new Rect(0, 0, -1, 1), new Rect(0, 0, 1, -1),
        };
        foreach (var rect in outside)
            Throws<ArgumentException>(() => target.ReadPixels(rect, 0, 0), $"rect {rect}", "Texture2D.ReadPixels");
        var unreadable = Filled(4, 4);
        unreadable.Apply(false, true);
        Throws<UnityException>(() => unreadable.ReadPixels(new Rect(0, 0, 2, 2), 0, 0), "unreadable", "ReadPixels");
        Ensure(Requests.Count == 0, "no bad call reached the host");
        Ensure(ComputeTestSupport.TakeWork(DisunityHost.WorldHandle).Length == 0, "no bad call recorded work");
    }

    private static void MipsFollowWhenAsked()
    {
        Begin(Mode.Answer);
        var source = new RenderTexture(4, 4, 0);
        RenderTexture.active = source;
        var target = Filled(4, 4, true);
        var updates = target.updateCount;
        target.ReadPixels(new Rect(0, 0, 4, 4), 0, 0, false);
        Near(Color.magenta, target.GetPixel(0, 0, 1), 1e-6f, "mip 1 keeps its texels without the flag");
        Ensure(target.updateCount == updates, "ReadPixels alone does not reach the GPU copy");
        var average = new Color((0 + 1 + 0 + 1) / 400f, (0 + 0 + 1 + 1) / 400f, 0.5f, 1f);
        target.ReadPixels(new Rect(0, 0, 4, 4), 0, 0, true);
        Near(average, target.GetPixel(0, 0, 1), 1e-6f, "mip 1 averages the new level 0");
        Ensure(target.updateCount == updates, "ReadPixels with the flag does not reach the GPU copy either");
        var fresh = Filled(4, 4, true);
        fresh.ReadPixels(new Rect(0, 0, 4, 4), 0, 0);
        Near(average, fresh.GetPixel(0, 0, 1), 1e-6f, "the overload without the flag regenerates the levels");
        target.Apply();
        Ensure(target.updateCount == updates + 1, "Apply does");
    }
}
