using Disunity.Hosting;
using UnityEngine;

/// <summary>The mesh renderers the window draws without a texture are reported by shader with the texture properties their
/// material holds, so a run that draws white shows which shader and which property carries the texture it expected.</summary>
internal static class WhiteRendererTests
{
    public static void Run()
    {
        try
        {
            OnlyBaseMapIsReportedWithItsType();
            Console.WriteLine("PASS: a renderer whose material holds only _BaseMap is reported under its shader with _BaseMap:Texture2D and a null mainTexture");
            PixelBackedMainTextureIsNotReported();
            Console.WriteLine("PASS: a renderer with a pixel-backed mainTexture is not reported");
            TextureKindsAreNamed();
            Console.WriteLine("PASS: a Texture2DArray, RenderTexture, Cubemap and Texture3D property is reported with its kind, and a 0x0 mainTexture reads as empty");
            SameShaderCountsEachRendererOnce();
            Console.WriteLine("PASS: two renderers with one shader count 2, and a renderer whose materials are set again counts once");
            LaterMaterialChangeIsReflected();
            Console.WriteLine("PASS: a mainTexture set after assignment leaves the renderer out of the report");
            RenderersWithoutMaterialAreCounted();
            Console.WriteLine("PASS: a renderer without a material is counted apart from the shaders");
            SummaryOrdersAndCutsAtTwelve();
            Console.WriteLine("PASS: the summary lists shaders by count then name with their properties, and cuts after 12 with 'and N more'");
            NewWorldStartsEmpty();
            Console.WriteLine("PASS: a new world starts with nothing reported");
            HostLinesNameTheShadersAndTotals();
            Console.WriteLine("PASS: the host's lines give the shaders and the totals");
        }
        finally
        {
            if (DisunityHost.IsLoaded) DisunityHost.UnloadWorld();
        }
    }

    private static void FreshWorld()
    {
        if (DisunityHost.IsLoaded) DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
    }

    private static MeshRenderer Renderer(Material? material)
    {
        var renderer = DisunityHost.CreateGameObject().AddComponent<MeshRenderer>();
        if (material is not null) renderer.sharedMaterial = material;
        return renderer;
    }

    private static Material MaterialOf(string shader, string? property = null, Texture? texture = null)
    {
        var material = new Material(DisunityHost.RegisterShader(shader));
        if (property is not null) material.SetTexture(property, texture!);
        return material;
    }

    private static WhiteRenderers.Group Only(WhiteRenderers.Report report)
    {
        Expect.Equal(1, report.Groups.Count, "one shader reported");
        return report.Groups[0];
    }

    private static void OnlyBaseMapIsReportedWithItsType()
    {
        FreshWorld();
        Renderer(MaterialOf("Test/White/Base", "_BaseMap", new Texture2D(4, 4)));
        var report = WhiteRenderers.Survey();
        var group = Only(report);
        Expect.Equal("Test/White/Base", group.Shader, "the shader");
        Expect.Equal(1, group.Count, "the count");
        Expect.Sequence(new[] { "_BaseMap:Texture2D" }, group.Properties, "the texture properties");
        Expect.Equal("null", group.MainTexture, "mainTexture");
        Expect.Equal(1, report.Total, "the total");
        Expect.Equal(0, report.NoMaterial, "no renderer lacks a material");
    }

    private static void PixelBackedMainTextureIsNotReported()
    {
        FreshWorld();
        var material = MaterialOf("Test/White/Textured");
        material.mainTexture = new Texture2D(4, 4);
        Renderer(material);
        var report = WhiteRenderers.Survey();
        Expect.Equal(0, report.Total, "a textured renderer is not white");
        Expect.Equal(1, report.Examined, "the renderer was examined");
        Expect.That(report.Groups.Count == 0 && report.Summary is null, "nothing to report");
    }

    private static void TextureKindsAreNamed()
    {
        FreshWorld();
        var material = MaterialOf("Test/White/Kinds", "_Albedo", new Texture2DArray(2, 2, 2, TextureFormat.RGBA32, false));
        material.SetTexture("_Target", new RenderTexture(4, 4, 0));
        material.SetTexture("_Env", new Cubemap(4, TextureFormat.RGBA32, false));
        material.SetTexture("_Volume", new Texture3D(2, 2, 2, TextureFormat.RGBA32, false));
        material.SetTexture("_Unset", (Texture)null!);
        material.mainTexture = new Texture2D(0, 0);
        Renderer(material);
        var group = Only(WhiteRenderers.Survey());
        Expect.Sequence(
            new[] { "_Albedo:Texture2DArray", "_Env:Cubemap", "_MainTex:Texture2D", "_Target:RenderTexture", "_Volume:Texture3D" },
            group.Properties, "each held texture with its kind, a null one left out");
        Expect.Equal("empty", group.MainTexture, "a Texture2D of size 0");
    }

    private static void SameShaderCountsEachRendererOnce()
    {
        FreshWorld();
        var first = Renderer(MaterialOf("Test/White/Pair", "_BaseMap", new Texture2D(4, 4)));
        Renderer(MaterialOf("Test/White/Pair", "_BaseMap", new Texture2D(4, 4)));
        first.sharedMaterial = first.sharedMaterial;
        var report = WhiteRenderers.Survey();
        Expect.Equal(2, report.Total, "two renderers, the reassigned one once");
        Expect.Equal(2, Only(report).Count, "two renderers of one shader");
    }

    private static void LaterMaterialChangeIsReflected()
    {
        FreshWorld();
        var material = MaterialOf("Test/White/Late", "_BaseMap", new Texture2D(4, 4));
        Renderer(material);
        Expect.Equal(1, WhiteRenderers.Survey().Total, "white while the material has no pixels");
        material.mainTexture = new Texture2D(4, 4);
        Expect.Equal(0, WhiteRenderers.Survey().Total, "not white once the material has pixels");
        Expect.Equal(1, WhiteRenderers.Survey().Examined, "still examined");
    }

    private static void RenderersWithoutMaterialAreCounted()
    {
        FreshWorld();
        Renderer(null);
        Renderer(null);
        Renderer(MaterialOf("Test/White/Some", "_BaseMap", new Texture2D(4, 4)));
        var report = WhiteRenderers.Survey();
        Expect.Equal(3, report.Total, "every renderer drawn without a texture");
        Expect.Equal(2, report.NoMaterial, "two have no material");
        Expect.Equal(1, Only(report).Count, "the shader groups hold only the renderers with a material");
        Expect.Equal(3, report.Examined, "three mesh renderers");
    }

    private static void SummaryOrdersAndCutsAtTwelve()
    {
        FreshWorld();
        Renderer(MaterialOf("Test/White/Many01", "_B", new Texture2D(4, 4)));
        Renderer(MaterialOf("Test/White/Many01", "_B", new Texture2D(4, 4)));
        Renderer(MaterialOf("Test/White/Many01", "_B", new Texture2D(4, 4)));
        Renderer(MaterialOf("Test/White/Many02", "_Z", new Texture2D(4, 4)));
        Renderer(MaterialOf("Test/White/Many02", "_A", new Texture2D(4, 4)));
        for (var number = 3; number <= 14; number++) Renderer(MaterialOf($"Test/White/Many{number:00}"));
        var summary = WhiteRenderers.Survey().Summary!;
        var items = summary.Split(", ");
        Expect.Equal("Test/White/Many01 3 (_B:Texture2D; mainTexture null)", items[0], "the largest group first");
        Expect.Equal("Test/White/Many02 2 (_A:Texture2D _Z:Texture2D; mainTexture null)", items[1], "properties of a shader are joined");
        Expect.Equal("Test/White/Many03 1 (no texture properties; mainTexture null)", items[2], "a material without textures");
        Expect.Equal(13, items.Length, "twelve shaders and the remainder");
        Expect.Equal("Test/White/Many12 1 (no texture properties; mainTexture null)", items[11], "the twelfth shader");
        Expect.Equal("and 2 more", items[12], "the shaders cut");
    }

    private static void NewWorldStartsEmpty()
    {
        FreshWorld();
        Renderer(MaterialOf("Test/White/Gone", "_BaseMap", new Texture2D(4, 4)));
        Expect.Equal(1, WhiteRenderers.Survey().Total, "reported in the first world");
        FreshWorld();
        Expect.Equal(0, WhiteRenderers.Survey().Total, "the next world reports nothing");
        Expect.Equal(0, WhiteRenderers.Survey().Examined, "and has no renderer");
    }

    private static void HostLinesNameTheShadersAndTotals()
    {
        FreshWorld();
        Expect.Equal(0, DisunityHost.WhiteRendererLines().Count, "no mesh renderer, no lines");
        Renderer(MaterialOf("Test/White/Line", "_BaseMap", new Texture2D(4, 4)));
        Renderer(null);
        Expect.Sequence(
            new[]
            {
                "renderers drawn without a texture by shader: Test/White/Line 1 (_BaseMap:Texture2D; mainTexture null)",
                "renderers drawn without a texture: 2 of 2 mesh renderers, 1 with no material",
            },
            DisunityHost.WhiteRendererLines(), "the host's lines");
    }
}
