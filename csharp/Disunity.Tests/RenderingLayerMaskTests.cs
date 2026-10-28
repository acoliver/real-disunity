using UnityEngine;

/// <summary><c>RenderingLayerMask</c> converts to and from the integers a game does arithmetic on
/// and names the one layer the engine defines, <c>Default</c>.</summary>
internal static class RenderingLayerMaskTests
{
    public static void Run()
    {
        ConvertsToAndFromIntegers();
        NamesTheDefaultLayer();
        BuildsMasksFromNames();
        Console.WriteLine("PASS: RenderingLayerMask conversions, default layer and name lookups");
    }

    private static void ConvertsToAndFromIntegers()
    {
        RenderingLayerMask fromUnsigned = 0x8000_0005u;
        Expect.Equal(0x8000_0005u, fromUnsigned.value, "uint to mask");
        Expect.Equal(0x8000_0005u, (uint)fromUnsigned, "mask to uint");
        RenderingLayerMask fromSigned = -1;
        Expect.Equal(uint.MaxValue, fromSigned.value, "a negative int sets every layer");
        Expect.Equal(-2147483643, (int)fromUnsigned, "mask to int keeps the bit pattern");
        Expect.Equal(0u, default(RenderingLayerMask).value, "the default value selects no layer");
        Expect.Equal(1u, RenderingLayerMask.defaultRenderingLayerMask.value, "the default mask is layer 0");
        var mutable = new RenderingLayerMask { value = 6 };
        mutable.value |= 1;
        Expect.Equal(7u, (uint)mutable, "the value is settable");
    }

    private static void NamesTheDefaultLayer()
    {
        Expect.Equal(0, RenderingLayerMask.NameToRenderingLayer("Default"), "Default is layer 0");
        Expect.Equal(-1, RenderingLayerMask.NameToRenderingLayer("No such layer"), "an unknown name is -1, as Unity documents");
        Expect.Equal(-1, RenderingLayerMask.NameToRenderingLayer("default"), "names are case sensitive");
        Expect.Equal("Default", RenderingLayerMask.RenderingLayerToName(0), "layer 0 is named Default");
        Expect.Equal(32, RenderingLayerMask.GetRenderingLayerCount(), "a project has 32 rendering layers");
        Expect.Equal(1, RenderingLayerMask.GetDefinedRenderingLayerCount(), "one layer is defined");
        Expect.Equal(0, RenderingLayerMask.GetLastDefinedRenderingLayerIndex(), "the last defined layer is 0");
        Expect.Equal(1u, RenderingLayerMask.GetDefinedRenderingLayersCombinedMaskValue(), "the defined layers combine to bit 0");
        Expect.Sequence(new[] { "Default" }, RenderingLayerMask.GetDefinedRenderingLayerNames(), "defined names");
        Expect.Sequence(new[] { 0 }, RenderingLayerMask.GetDefinedRenderingLayerValues(), "defined values");
        RenderingLayerMask.GetDefinedRenderingLayerNames()[0] = "Changed";
        Expect.Equal("Default", RenderingLayerMask.RenderingLayerToName(0), "the returned names are a copy");
    }

    private static void BuildsMasksFromNames()
    {
        Expect.Equal(1u, RenderingLayerMask.GetMask("Default"), "the mask of Default");
        Expect.Equal(1u, RenderingLayerMask.GetMask("Default", "Default"), "repeated names set their bit once");
        Expect.Equal(0u, RenderingLayerMask.GetMask(), "no names give an empty mask");
        Expect.Equal(1u, RenderingLayerMask.GetMask((ReadOnlySpan<string>)["Default"]), "a span of names");
    }
}
