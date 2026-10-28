namespace UnityEngine;

/// <summary>
/// A mask of rendering layers, which lights and decals use to choose the renderers they
/// affect. The engine defines Unity's default set: layer 0, <c>Default</c>, of 32. The names a
/// project gives other layers are not read from the player yet.
/// </summary>
public struct RenderingLayerMask
{
    private const int Count = 32;
    private static readonly string[] Names = ["Default"];

    public uint value { get; set; }

    public static RenderingLayerMask defaultRenderingLayerMask => 1u;

    public static implicit operator uint(RenderingLayerMask mask) => mask.value;
    public static implicit operator RenderingLayerMask(uint intVal) => new() { value = intVal };
    public static implicit operator int(RenderingLayerMask mask) => unchecked((int)mask.value);
    public static implicit operator RenderingLayerMask(int intVal) => new() { value = unchecked((uint)intVal) };

    public static string RenderingLayerToName(int layer) => layer >= 0 && layer < Names.Length ? Names[layer] : "";
    public static int NameToRenderingLayer(string layerName) => Array.IndexOf(Names, layerName);
    public static uint GetMask(params string[] renderingLayerNames) => GetMask((ReadOnlySpan<string>)renderingLayerNames);

    public static uint GetMask(ReadOnlySpan<string> renderingLayerNames)
    {
        var mask = 0u;
        foreach (var name in renderingLayerNames)
            if (NameToRenderingLayer(name) is var layer and >= 0) mask |= 1u << layer;
        return mask;
    }

    public static int GetDefinedRenderingLayerCount() => Names.Length;
    public static int GetLastDefinedRenderingLayerIndex() => Names.Length - 1;
    public static uint GetDefinedRenderingLayersCombinedMaskValue() => (1u << Names.Length) - 1;
    public static string[] GetDefinedRenderingLayerNames() => (string[])Names.Clone();
    public static int[] GetDefinedRenderingLayerValues() => Enumerable.Range(0, Names.Length).ToArray();
    public static int GetRenderingLayerCount() => Count;
}
