using Disunity.Hosting;
using UnityEngine;

/// <summary>The shared material a UI graphic falls back to.</summary>
internal static class DefaultCanvasMaterialTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Shared();
            Console.WriteLine("PASS: Canvas.GetDefaultCanvasMaterial returns one shared material on the UI/Default shader at queue 3000 with one pass");
            Etc1();
            Console.WriteLine("PASS: Canvas.GetETC1SupportedCanvasMaterial returns the same default material");
            DeclaresMaskProperties();
            Console.WriteLine("PASS: the default canvas material declares the stencil and colour mask properties with their defaults");
            Recreated();
            Console.WriteLine("PASS: a destroyed default canvas material is replaced by a new one");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void Shared()
    {
        var material = Canvas.GetDefaultCanvasMaterial();
        Expect.That(material, "a material is returned");
        Expect.That(ReferenceEquals(material, Canvas.GetDefaultCanvasMaterial()), "the same material each call");
        Expect.Equal("UI/Default", material.shader.name, "shader name");
        Expect.Equal(3000, material.renderQueue, "render queue");
        Expect.Equal(1, material.shader.passCount, "pass count");
        Expect.That(ReferenceEquals(material.shader, Shader.Find("UI/Default")), "the shader is the registered UI/Default");
    }

    private static void DeclaresMaskProperties()
    {
        var material = Canvas.GetDefaultCanvasMaterial();
        foreach (var name in new[] { "_Stencil", "_StencilOp", "_StencilComp", "_StencilReadMask", "_StencilWriteMask", "_ColorMask", "_UseUIAlphaClip" })
            Expect.That(material.HasProperty(name), name + " is declared");
        Expect.Equal(8, material.GetInt("_StencilComp"), "stencil comparison defaults to always");
        Expect.Equal(255, material.GetInt("_StencilReadMask"), "stencil read mask");
        Expect.Equal(255, material.GetInt("_StencilWriteMask"), "stencil write mask");
        Expect.Equal(15, material.GetInt("_ColorMask"), "all colour channels are written");
        Expect.Equal(0, material.GetInt("_Stencil"), "stencil reference");
        Expect.Equal(0, material.GetInt("_StencilOp"), "stencil operation keeps");
        var copy = new Material(material);
        Expect.That(copy.HasProperty("_ColorMask") && copy.GetInt("_ColorMask") == 15, "a copy keeps the properties");
    }

    private static void Etc1() =>
        Expect.That(ReferenceEquals(Canvas.GetETC1SupportedCanvasMaterial(), Canvas.GetDefaultCanvasMaterial()), "ETC1 material is the default");

    private static void Recreated()
    {
        var first = Canvas.GetDefaultCanvasMaterial();
        UnityEngine.Object.DestroyImmediate(first);
        var second = Canvas.GetDefaultCanvasMaterial();
        Expect.That(second && !ReferenceEquals(first, second), "a new material replaces the destroyed one");
        Expect.Equal(3000, second.renderQueue, "the new material has the same queue");
    }
}
