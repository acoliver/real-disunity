using Disunity.Hosting;
using UnityEngine;

/// <summary>A texture the engine cannot send to the world arrives there as none. The run reports each property and the reason, so
/// a global that reads as undeclared can be traced to the texture the game set.</summary>
internal static class UnsentTextureTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            DestroyedRenderTextureGlobalIsNoted();
            Console.WriteLine("PASS: a destroyed RenderTexture set as a global is noted with its property and destroyed-render-texture");
            EmptyTexture2DIsNoted();
            Console.WriteLine("PASS: a Texture2D of size 0 is noted as empty-texture2d with its size");
            DestroyedLayeredTexturesAreNotedByKind();
            Console.WriteLine("PASS: a destroyed Cubemap, Texture3D and Texture2DArray are noted as destroyed with the kind");
            SentTexturesAndNullAreSilent();
            Console.WriteLine("PASS: null, a live RenderTexture, a Texture2D with pixels and a stored array, cube or volume are not noted");
            RepeatedSetsCountPerPropertyAndReason();
            Console.WriteLine("PASS: repeated sets count under one property and reason, and a property or reason of its own counts apart");
            MaterialAndBlockPathsNameTheirProperty();
            Console.WriteLine("PASS: Material.SetTexture, mainTexture and MaterialPropertyBlock.SetTexture note the texture they could not send");
            SummaryListsEveryPropertyAndReason();
            Console.WriteLine("PASS: the summary lists each property, reason and count, and a new world starts it empty");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static int CountOf(string property, string reason) =>
        UnsentTextures.Entries.Where(e => e.Property == property && e.Reason == reason).Sum(e => e.Count);

    private static int CountFor(string property) => UnsentTextures.Entries.Where(e => e.Property == property).Sum(e => e.Count);

    private static RenderTexture DestroyedRenderTexture()
    {
        var target = new RenderTexture(4, 4, 0);
        UnityEngine.Object.DestroyImmediate(target);
        return target;
    }

    /// <summary>Clears a global so a world loaded later does not send it again.</summary>
    private static void Forget(string name) => Shader.SetGlobalTexture(name, (Texture)null!);

    private static void DestroyedRenderTextureGlobalIsNoted()
    {
        Shader.SetGlobalTexture("_GlobalTerrainLastHeightmap", DestroyedRenderTexture());
        Expect.Equal(1, CountOf("_GlobalTerrainLastHeightmap", "destroyed-render-texture"), "the destroyed render texture global");
        Expect.Equal(1, UnsentTextures.Entries.Count(e => e.Property == "_GlobalTerrainLastHeightmap"), "one entry for the property");
        Forget("_GlobalTerrainLastHeightmap");
    }

    private static void EmptyTexture2DIsNoted()
    {
        Shader.SetGlobalTexture("_EmptyGlobal", new Texture2D(0, 0));
        Expect.Equal(1, CountOf("_EmptyGlobal", "empty-texture2d:0x0"), "a 0x0 Texture2D");
        Shader.SetGlobalTexture("_EmptyWide", new Texture2D(8, 0));
        Expect.Equal(1, CountOf("_EmptyWide", "empty-texture2d:8x0"), "an 8x0 Texture2D names its size");
        Forget("_EmptyGlobal");
        Forget("_EmptyWide");
    }

    private static void DestroyedLayeredTexturesAreNotedByKind()
    {
        var cube = new Cubemap(4, TextureFormat.RGBA32, false);
        var volume = new Texture3D(2, 2, 2, TextureFormat.RGBA32, false);
        var array = new Texture2DArray(2, 2, 2, TextureFormat.RGBA32, false);
        foreach (var texture in new Texture[] { cube, volume, array }) UnityEngine.Object.DestroyImmediate(texture);
        Shader.SetGlobalTexture("_KindCube", cube);
        Shader.SetGlobalTexture("_KindVolume", volume);
        Shader.SetGlobalTexture("_KindArray", array);
        Expect.Equal(1, CountOf("_KindCube", "destroyed-cubemap"), "a Cubemap");
        Expect.Equal(1, CountOf("_KindVolume", "destroyed-texture3d"), "a Texture3D");
        Expect.Equal(1, CountOf("_KindArray", "destroyed-texture2darray"), "a Texture2DArray");
        foreach (var name in new[] { "_KindCube", "_KindVolume", "_KindArray" }) Forget(name);
    }

    private static void SentTexturesAndNullAreSilent()
    {
        Shader.SetGlobalTexture("_SilentNull", (Texture)null!);
        Shader.SetGlobalTexture("_SilentLive", new RenderTexture(4, 4, 0));
        Shader.SetGlobalTexture("_SilentPixels", new Texture2D(4, 4));
        Shader.SetGlobalTexture("_SilentCube", new Cubemap(4, TextureFormat.RGBA32, false));
        Shader.SetGlobalTexture("_SilentVolume", new Texture3D(2, 2, 2, TextureFormat.RGBA32, false));
        Shader.SetGlobalTexture("_SilentArray", new Texture2DArray(2, 2, 2, TextureFormat.RGBA32, false));
        foreach (var name in new[] { "_SilentNull", "_SilentLive", "_SilentPixels", "_SilentCube", "_SilentVolume", "_SilentArray" })
            Expect.Equal(0, CountFor(name), $"{name} is sent as a texture or as the absence of one");
        Forget("_SilentLive");
        Forget("_SilentPixels");
        foreach (var name in new[] { "_SilentCube", "_SilentVolume", "_SilentArray" }) Forget(name);
    }

    private static void RepeatedSetsCountPerPropertyAndReason()
    {
        for (var set = 0; set < 3; set++) Shader.SetGlobalTexture("_Repeated", DestroyedRenderTexture());
        Expect.Equal(3, CountOf("_Repeated", "destroyed-render-texture"), "three sets");
        Shader.SetGlobalTexture("_Repeated", new Texture2D(0, 0));
        Expect.Equal(3, CountOf("_Repeated", "destroyed-render-texture"), "another reason leaves the first count");
        Expect.Equal(1, CountOf("_Repeated", "empty-texture2d:0x0"), "another reason counts apart");
        Shader.SetGlobalTexture("_RepeatedOther", DestroyedRenderTexture());
        Expect.Equal(3, CountOf("_Repeated", "destroyed-render-texture"), "another property leaves the first count");
        Expect.Equal(1, CountOf("_RepeatedOther", "destroyed-render-texture"), "another property counts apart");
        Forget("_Repeated");
        Forget("_RepeatedOther");
    }

    private static void MaterialAndBlockPathsNameTheirProperty()
    {
        var material = new Material(DisunityHost.RegisterShader("Test/Unsent/Material"));
        _ = material.WorldLink;
        material.SetTexture("_Detail", DestroyedRenderTexture());
        Expect.Equal(1, CountOf("_Detail", "destroyed-render-texture"), "Material.SetTexture");
        material.mainTexture = new Texture2D(0, 0);
        Expect.Equal(1, CountOf("_MainTex", "empty-texture2d:0x0"), "Material.mainTexture");

        var before = new Material(DisunityHost.RegisterShader("Test/Unsent/Whole"));
        var early = new Cubemap(4, TextureFormat.RGBA32, false);
        UnityEngine.Object.DestroyImmediate(early);
        before.SetTexture("_Early", early);
        _ = before.WorldLink;
        Expect.Equal(1, CountOf("_Early", "destroyed-cubemap"), "a material sent whole");

        var renderer = DisunityHost.CreateGameObject().AddComponent<MeshRenderer>();
        var block = new MaterialPropertyBlock();
        block.SetTexture("_BlockTex", DestroyedRenderTexture());
        renderer.SetPropertyBlock(block);
        Expect.Equal(1, CountOf("_BlockTex", "destroyed-render-texture"), "MaterialPropertyBlock.SetTexture");
    }

    private static void SummaryListsEveryPropertyAndReason()
    {
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        Expect.That(UnsentTextures.Summary is null, $"a new world has nothing to report, got {UnsentTextures.Summary}");
        Shader.SetGlobalTexture("_SummaryB", new Texture2D(0, 0));
        for (var set = 0; set < 2; set++) Shader.SetGlobalTexture("_SummaryA", DestroyedRenderTexture());
        Expect.Equal("_SummaryA destroyed-render-texture 2, _SummaryB empty-texture2d:0x0 1", UnsentTextures.Summary, "the summary");
        Expect.Equal("_SummaryA destroyed-render-texture 2, _SummaryB empty-texture2d:0x0 1", DisunityHost.UnsentTextureSummary, "the host's summary");
        Forget("_SummaryA");
        Forget("_SummaryB");
    }
}
