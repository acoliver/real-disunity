using UnityEngine;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Graphics.SetRenderTarget with a render texture sets RenderTexture.active and draws nothing, once for each
/// argument count a compiled game can call.</summary>
internal static class GraphicsTargetTests
{
    public static void Run()
    {
        EachOverloadSetsAndClearsTheActiveTexture();
        MipLevelAndDepthSliceAreChecked();
        EveryArgumentCountIsItsOwnMethod();
        Console.WriteLine("PASS: SetRenderTarget sets and clears RenderTexture.active for every overload, rejects missing levels and slices");
    }

    private static RenderTexture Volume() => new(new RenderTextureDescriptor(8, 8, RenderTextureFormat.ARGB32, 0, 4)
    {
        dimension = TextureDimension.Tex2DArray, volumeDepth = 3, useMipMap = true, autoGenerateMips = false,
    });

    private static void EachOverloadSetsAndClearsTheActiveTexture()
    {
        var target = Volume();
        var calls = new Action[]
        {
            () => Graphics.SetRenderTarget(target),
            () => Graphics.SetRenderTarget(target, 1),
            () => Graphics.SetRenderTarget(target, 1, CubemapFace.Unknown),
            () => Graphics.SetRenderTarget(target, 1, CubemapFace.PositiveX, 2),
        };
        foreach (var call in calls)
        {
            RenderTexture.active = null;
            call();
            Expect.That(ReferenceEquals(RenderTexture.active, target), "the target becomes active");
            Expect.That(target.IsCreated(), "and is created");
            Graphics.SetRenderTarget(null!);
            Expect.That(RenderTexture.active == null, "null restores the screen");
        }
        Graphics.SetRenderTarget(target, 2);
        Graphics.SetRenderTarget(null!, 5, CubemapFace.Unknown, 9);
        Expect.That(RenderTexture.active == null, "the engine does not check a mip level or slice when restoring the screen");
    }

    private static void MipLevelAndDepthSliceAreChecked()
    {
        var target = Volume();
        RenderTexture.active = null;
        Throws<ArgumentException>(() => Graphics.SetRenderTarget(target, 4), "a mip level past the chain", "mip level 4", "0 to 3");
        Throws<ArgumentException>(() => Graphics.SetRenderTarget(target, -1), "a negative mip level", "mip level -1");
        Throws<ArgumentException>(() => Graphics.SetRenderTarget(target, 0, CubemapFace.Unknown, 3), "a slice past the depth", "depth slice 3", "0 to 2");
        Throws<ArgumentException>(() => Graphics.SetRenderTarget(target, 0, CubemapFace.Unknown, -1), "a negative slice", "depth slice -1");
        Expect.That(RenderTexture.active == null, "a rejected call leaves the active texture alone");
        var plain = new RenderTexture(4, 4, 0);
        Graphics.SetRenderTarget(plain);
        Throws<ArgumentException>(() => Graphics.SetRenderTarget(plain, 1), "a texture without mips has one level", "mip level 1", "0 to 0");
        Throws<ArgumentException>(() => Graphics.SetRenderTarget(plain, 0, CubemapFace.Unknown, 1), "a 2D texture has one slice", "depth slice 1");
        RenderTexture.active = null;
    }

    private static void EveryArgumentCountIsItsOwnMethod()
    {
        var counts = typeof(Graphics).GetMethods().Where(method => method.Name == "SetRenderTarget" && method.GetParameters()[0].ParameterType == typeof(RenderTexture))
            .Select(method => method.GetParameters()).ToList();
        Expect.Sequence([1, 2, 3, 4], counts.Select(parameters => parameters.Length).Order(), "a compiled call finds the overload of its argument count");
        Expect.That(counts.All(parameters => parameters.All(parameter => !parameter.IsOptional)), "no overload relies on optional parameters");
    }
}
