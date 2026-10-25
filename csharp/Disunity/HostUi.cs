using Disunity.Native;
using UnityEngine;
using UnityEngine.UI;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    internal static uint ViewportWidth { get; private set; } = 800;
    internal static uint ViewportHeight { get; private set; } = 600;
    public static void SetViewport(uint width, uint height)
    {
        if (width == 0 || height == 0 || width > 16384 || height > 16384) throw new ArgumentOutOfRangeException(nameof(width));
        ViewportWidth = width;
        ViewportHeight = height;
    }
    public static unsafe Font RegisterFont(byte[] bytes)
    {
        ArgumentNullException.ThrowIfNull(bytes);
        ulong handle;
        fixed (byte* pointer = bytes)
            ThrowIfError(NativeBindings.disunity_font_register(WorldHandle, pointer, checked((uint)bytes.Length), &handle));
        return new Font(handle, WorldHandle);
    }
    public static unsafe Text CreateText(GameObject owner, Font font)
    {
        ArgumentNullException.ThrowIfNull(font);
        if (font.World != WorldHandle) throw new ArgumentException("font belongs to another world");
        ulong handle;
        ThrowIfError(NativeBindings.disunity_ui_create(WorldHandle, owner.NativeHandle, 2, font.Handle, &handle));
        var text = new Text { Handle = handle, gameObject = owner };
        NativeComponents.Add(handle, text);
        return text;
    }
}
