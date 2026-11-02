using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;

/// <summary>A texture's bytes reach the host when it takes a renderer identity and again on Apply.</summary>
internal static class PlayerTextureStoreTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            StoresTheBytesWhenTheTextureTakesAnIdentity();
            StoresAgainOnApplyAndOnlyOnceAnIdentityExists();
            ReportsSrgbTexturesAsNotLinear();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static unsafe (DisunityTextureInfo Info, byte[] Bytes) Read(ulong identity)
    {
        var info = new DisunityTextureInfo();
        DisunityHost.ThrowIfError(NativeBindings.disunity_texture_read(DisunityHost.WorldHandle, identity, &info, null, 0));
        var bytes = new byte[info.length];
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_texture_read(DisunityHost.WorldHandle, identity, &info, pointer, info.length));
        return (info, bytes);
    }

    private static Texture2D TwoPixelTexture(bool linear)
    {
        var texture = new Texture2D(2, 1, TextureFormat.RGBA32, false, linear);
        texture.SetPixel(0, 0, new Color32(10, 20, 30, 40));
        texture.SetPixel(1, 0, new Color32(50, 60, 70, 80));
        return texture;
    }

    private static void StoresTheBytesWhenTheTextureTakesAnIdentity()
    {
        var texture = TwoPixelTexture(linear: true);
        var identity = texture.RendererIdentity;
        var (info, bytes) = Read(identity);
        Expect.That(info.present == 1 && info.generation == 1, "taking an identity stores the texture once");
        Expect.That(info.width == 2 && info.height == 1 && info.mips == 1 && info.format == (uint)TextureFormat.RGBA32 && info.linear == 1,
            $"the stored description carries the size, mips, format and linear flag, got {info.width}x{info.height} format {info.format}");
        Expect.Sequence(texture.GetRawTextureData(), bytes, "the stored bytes are the texture's raw data");
        Expect.Sequence(new byte[] { 10, 20, 30, 40, 50, 60, 70, 80 }, bytes, "the raw data holds the pixels set");
        Expect.That(texture.RendererIdentity == identity && Read(identity).Info.generation == 1, "reading the identity again stores nothing");
        Console.WriteLine("PASS: a texture stores its bytes with its description when it takes a renderer identity");
    }

    private static void StoresAgainOnApplyAndOnlyOnceAnIdentityExists()
    {
        var withIdentity = TwoPixelTexture(linear: true);
        var identity = withIdentity.RendererIdentity;
        withIdentity.SetPixel(0, 0, new Color32(1, 2, 3, 4));
        Expect.That(Read(identity).Info.generation == 1, "changing pixels stores nothing until Apply");
        withIdentity.Apply();
        var (info, bytes) = Read(identity);
        Expect.That(info.generation == 2, "Apply stores a texture that has an identity again");
        Expect.Sequence(new byte[] { 1, 2, 3, 4, 50, 60, 70, 80 }, bytes, "Apply stores the changed bytes");
        var probe = TwoPixelTexture(linear: true).RendererIdentity;
        TwoPixelTexture(linear: true).Apply();
        var next = TwoPixelTexture(linear: true).RendererIdentity;
        Expect.That(next == probe + 1, "Apply on a texture without an identity does not give it one");
        Console.WriteLine("PASS: Apply stores a texture again once it has an identity and gives none");
    }

    private static void ReportsSrgbTexturesAsNotLinear()
    {
        var info = Read(TwoPixelTexture(linear: false).RendererIdentity).Info;
        Expect.That(info.linear == 0, "an sRGB texture is stored with the linear flag clear");
        Console.WriteLine("PASS: an sRGB texture is stored as not linear");
    }
}
