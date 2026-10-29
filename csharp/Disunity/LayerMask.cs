namespace UnityEngine;

public struct LayerMask
{
    private static readonly string[] Names = BuiltInNames();

    public int value { get; set; }
    public static implicit operator int(LayerMask mask) => mask.value;
    public static implicit operator LayerMask(int value) => new() { value = value };

    private static string[] BuiltInNames()
    {
        var names = new string[32];
        names[0] = "Default";
        names[1] = "TransparentFX";
        names[2] = "Ignore Raycast";
        names[4] = "Water";
        names[5] = "UI";
        return names;
    }

    /// <summary>Names a user layer (8 to 31) for a host or test that has no plan names.</summary>
    internal static void SetUserLayerName(int layer, string name)
    {
        if (layer is < 8 or > 31) throw new ArgumentOutOfRangeException(nameof(layer), "user layers are 8 to 31");
        Names[layer] = name;
    }

    /// <summary>
    /// Replaces the names of all 32 layers with a project's, by layer index. A project can name the layers
    /// below 8 that the built-in names leave empty, so every slot is replaced; layers past the end of the
    /// list are unnamed.
    /// </summary>
    internal static void SetLayerNames(IReadOnlyList<string> names)
    {
        ArgumentNullException.ThrowIfNull(names);
        if (names.Count > 32) throw new ArgumentException($"there are 32 layers, not {names.Count}", nameof(names));
        for (var layer = 0; layer < 32; layer++) Names[layer] = layer < names.Count ? names[layer] ?? "" : "";
    }

    /// <summary>Restores the built-in names and forgets every other layer name.</summary>
    internal static void ClearUserLayerNames() => BuiltInNames().CopyTo(Names, 0);

    /// <summary>The layer with the name (matched exactly), or -1.</summary>
    public static int NameToLayer(string layerName)
    {
        ArgumentNullException.ThrowIfNull(layerName);
        return layerName == "" ? -1 : Array.IndexOf(Names, layerName);
    }

    /// <summary>The name of the layer, or an empty string for an unnamed layer or one outside 0 to 31.</summary>
    public static string LayerToName(int layer) => layer is >= 0 and < 32 ? Names[layer] ?? "" : "";

    /// <summary>A mask with the bit of each named layer set; a name that is not a layer adds nothing.</summary>
    public static int GetMask(params string[] layerNames)
    {
        ArgumentNullException.ThrowIfNull(layerNames);
        var mask = 0;
        foreach (var name in layerNames)
            if (NameToLayer(name) is var layer and >= 0) mask |= 1 << layer;
        return mask;
    }
}
