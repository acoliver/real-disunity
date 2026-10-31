using System.Text;
using Disunity.Hosting;
using UnityEngine;

/// <summary>Material.ComputeCRC: a CRC-32 over the shader name, queue, keywords, override tags and every set property.</summary>
internal static class MaterialCrcTests
{
    public static void Run()
    {
        TheCrcMatchesTheStandardCheckValue();
        Console.WriteLine("PASS: the CRC-32 gives the standard check value and Animator.StringToHash still uses it");
        EqualMaterialsHaveEqualValues();
        Console.WriteLine("PASS: ComputeCRC is stable across calls and equal for equal materials whatever order they were set up in");
        EachKindOfChangeChangesIt();
        Console.WriteLine("PASS: ComputeCRC changes with the shader, queue, keywords, override tags and every kind of property");
    }

    private static Material Fresh(string shader = "Test/MaterialCrc/A") => new(DisunityHost.RegisterShader(shader));

    private static void TheCrcMatchesTheStandardCheckValue()
    {
        Expect.Equal(0xCBF43926u, Crc32.Of(Encoding.ASCII.GetBytes("123456789")), "the CRC-32 check value");
        Expect.Equal(0u, Crc32.Of([]), "the CRC-32 of nothing");
        Expect.Equal(unchecked((int)0xCBF43926u), Animator.StringToHash("123456789"), "state names hash to the same CRC");
    }

    private static void EqualMaterialsHaveEqualValues()
    {
        var a = Fresh();
        Expect.Equal(a.ComputeCRC(), a.ComputeCRC(), "two calls on one material");
        var b = Fresh();
        Expect.Equal(a.ComputeCRC(), b.ComputeCRC(), "two plain materials of one shader");
        a.SetFloat("_X", 1f); a.SetColor("_C", Color.red); a.EnableKeyword("K1"); a.EnableKeyword("K2"); a.SetOverrideTag("T1", "v"); a.SetOverrideTag("T2", "w");
        b.SetOverrideTag("T2", "w"); b.EnableKeyword("K2"); b.SetColor("_C", Color.red); b.EnableKeyword("K1"); b.SetOverrideTag("T1", "v"); b.SetFloat("_X", 1f);
        Expect.Equal(a.ComputeCRC(), b.ComputeCRC(), "the order of setting does not matter");
        Expect.Equal(a.ComputeCRC(), new Material(a).ComputeCRC(), "a copy has the same value");
        var shader = DisunityHost.RegisterShader("Test/MaterialCrc/A");
        Expect.Equal(new Material(shader).ComputeCRC(), new Material(shader).ComputeCRC(), "a second pair");
    }

    private static void Differs(string what, Action<Material> change)
    {
        var material = Fresh();
        var before = material.ComputeCRC();
        change(material);
        Expect.That(material.ComputeCRC() != before, $"{what} changes the value");
    }

    private static void EachKindOfChangeChangesIt()
    {
        Differs("the shader", m => m.shader = DisunityHost.RegisterShader("Test/MaterialCrc/B"));
        Differs("the render queue", m => m.renderQueue = 3333);
        Differs("a keyword", m => m.EnableKeyword("FOG"));
        Differs("an override tag", m => m.SetOverrideTag("RenderType", "Opaque"));
        Differs("a float", m => m.SetFloat("_F", 2f));
        Differs("an integer", m => m.SetInteger("_I", 3));
        Differs("a vector", m => m.SetVector("_V", new Vector4(1, 2, 3, 4)));
        Differs("a color", m => m.SetColor("_Col", Color.green));
        Differs("a matrix", m => m.SetMatrix("_M", Matrix4x4.Scale(new Vector3(2, 2, 2))));
        Differs("a texture", m => m.SetTexture("_T", new Texture2D(2, 2)));
        Differs("a texture offset", m => m.SetTextureOffset("_T", new Vector2(1, 1)));
        Differs("a texture scale", m => m.SetTextureScale("_T", new Vector2(3, 3)));
        Differs("a float array", m => m.SetFloatArray("_FA", new[] { 1f, 2f }));
        Differs("a vector array", m => m.SetVectorArray("_VA", new[] { Vector4.one }));
        var a = Fresh(); var b = Fresh();
        a.SetFloat("_X", 1f); b.SetFloat("_X", 2f);
        Expect.That(a.ComputeCRC() != b.ComputeCRC(), "a different float value differs");
        a = Fresh(); b = Fresh();
        a.SetFloat("_X", 1f); b.SetFloat("_Y", 1f);
        Expect.That(a.ComputeCRC() != b.ComputeCRC(), "a different property name differs");
        a = Fresh(); b = Fresh();
        a.SetFloat("_X", 0f); b.SetInteger("_X", 0);
        Expect.That(a.ComputeCRC() != b.ComputeCRC(), "a different property kind differs");
        a = Fresh(); b = Fresh();
        a.EnableKeyword("A"); a.EnableKeyword("B"); a.DisableKeyword("B");
        Expect.Equal(b.ComputeCRC() == a.ComputeCRC(), false, "a keyword left enabled differs");
        a.DisableKeyword("A");
        Expect.Equal(b.ComputeCRC(), a.ComputeCRC(), "a keyword turned off again leaves the value");
    }
}
