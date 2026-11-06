using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using UnityEngine.VFX;
using Unity.IO.LowLevel.Unsafe;

/// <summary>The cubemap, legacy keyword, read handle and VFX settings types a render pipeline package needs to load.</summary>
internal static class PipelineTypeTests
{
    public static void Run()
    {
        CubemapHoldsASizeAndAFormat();
        Console.WriteLine("PASS: Cubemap holds a size, a format and a mip count, and refuses face pixels");
        KeywordsAreByName();
        Console.WriteLine("PASS: ShaderKeyword is a global keyword by name with a registration index");
        HandlesAreNeverValid();
        Console.WriteLine("PASS: FileHandle and ReadHandle are the invalid default and VFXCameraXRSettings is plain data");
    }

    private static void CubemapHoldsASizeAndAFormat()
    {
        var chain = new Cubemap(64, TextureFormat.RGBA32, true);
        Expect.That(chain.width == 64 && chain.height == 64 && chain.dimension == TextureDimension.Cube, "size and dimension");
        Expect.Equal(7, chain.mipmapCount, "a full chain of 64 has 7 levels");
        Expect.Equal(TextureFormat.RGBA32, chain.format, "format");
        Expect.Equal(1, new Cubemap(64, TextureFormat.RGBA32, false).mipmapCount, "no chain is one level");
        Expect.Equal(3, new Cubemap(64, TextureFormat.RGBA32, 3).mipmapCount, "a mip count");
        Expect.Equal(7, new Cubemap(64, TextureFormat.RGBA32, 99).mipmapCount, "a mip count past the chain is clamped");
        var fromGraphics = new Cubemap(8, GraphicsFormat.R8G8B8A8_UNorm, TextureCreationFlags.MipChain);
        Expect.That(fromGraphics.mipmapCount == 4 && fromGraphics.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm, "from a graphics format with a chain");
        Expect.Equal(1, new Cubemap(8, GraphicsFormat.R8G8B8A8_UNorm, TextureCreationFlags.None).mipmapCount, "from a graphics format without one");
        Expect.Throws<ArgumentException>(() => _ = new Cubemap(0, TextureFormat.RGBA32, false), "size 0");
        Expect.Throws<ArgumentException>(() => _ = new Cubemap(16385, TextureFormat.RGBA32, false), "size past the limit");
        Expect.Throws<ArgumentException>(() => _ = new Cubemap(8, TextureFormat.RGBA32, 0), "mip count 0");
        foreach (var (name, call) in new (string, Action)[]
        {
            ("GetPixel", () => chain.GetPixel(CubemapFace.PositiveX, 0, 0)),
            ("SetPixel", () => chain.SetPixel(CubemapFace.PositiveX, 0, 0, Color.red)),
            ("GetPixels", () => chain.GetPixels(CubemapFace.NegativeZ)),
            ("SetPixels", () => chain.SetPixels(new Color[1], CubemapFace.NegativeZ)),
            ("Apply", () => chain.Apply()),
        })
        {
            var error = Expect.Catch<NotSupportedException>(call, name);
            Expect.That(error.Message.Contains($"Cubemap.{name}"), $"{name} is named: {error.Message}");
        }
    }

    private static void KeywordsAreByName()
    {
        var a = new ShaderKeyword("_R59_A");
        var b = new ShaderKeyword("_R59_B");
        var again = new ShaderKeyword("_R59_A");
        Expect.That(a.name == "_R59_A" && a.IsValid() && a.GetKeywordType() == ShaderKeywordType.UserDefined, "a named keyword");
        Expect.That(a == again && a != b && a.Equals((object)again) && a.GetHashCode() == again.GetHashCode(), "equal by name");
        Expect.That(a.index >= 0 && b.index == a.index + 1 && again.index == a.index, "registration order");
        Expect.That(!default(ShaderKeyword).IsValid() && default(ShaderKeyword).name == "" && default(ShaderKeyword).index == -1, "a default keyword is invalid");
        Expect.Equal(ShaderKeywordType.None, default(ShaderKeyword).GetKeywordType(), "and has no type");
        Expect.Throws<ArgumentNullException>(() => _ = new ShaderKeyword(null!), "a null name");
        Expect.Equal("_R59_B", b.ToString(), "ToString is the name");
    }

    private static void HandlesAreNeverValid()
    {
        var file = default(FileHandle);
        var read = default(ReadHandle);
        Expect.That(!file.IsValid() && file.Status == FileState.Absent && file.JobHandle.IsCompleted && file.Close().IsCompleted, "a file handle");
        Expect.That(!read.IsValid() && read.Status == ReadStatus.Failed && read.GetBytesRead() == 0 && read.JobHandle.IsCompleted, "a read handle");
        read.Cancel();
        read.Dispose();
        var settings = new VFXCameraXRSettings { viewTotal = 2, viewCount = 1, viewOffset = 1 };
        Expect.That(settings.viewTotal == 2 && settings.viewCount == 1 && settings.viewOffset == 1, "plain data");
    }
}
