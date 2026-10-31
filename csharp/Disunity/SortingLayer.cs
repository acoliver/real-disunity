namespace UnityEngine;

/// <summary>
/// A sorting layer of the running game: a name and an id, drawn in table order. The engine starts with the
/// Default layer (id 0, value 0) alone; a player's scene plan replaces the table with the player's layers.
/// The value of a layer is its position relative to Default, so layers drawn before Default are negative.
/// </summary>
public struct SortingLayer
{
    private static readonly List<(string Name, int Id)> Table = [("Default", 0)];
    private static readonly HashSet<string> Warned = [];

    private int _id;

    /// <summary>The id renderers store for this layer.</summary>
    public readonly int id => _id;

    /// <summary>The layer's name, or an empty string when the id is not registered.</summary>
    public readonly string name => IDToName(_id);

    /// <summary>The layer's position in drawing order relative to Default, or 0 when the id is not registered.</summary>
    public readonly int value => GetLayerValueFromID(_id);

    /// <summary>The registered layers in drawing order, first drawn first.</summary>
    public static SortingLayer[] layers => Table.Select(entry => new SortingLayer { _id = entry.Id }).ToArray();

    /// <summary>The id of the layer with the name (matched exactly), or 0 when no layer has it.</summary>
    public static int NameToID(string name) => TryFind(name, out var id) ? id : 0;

    /// <summary>The name of the layer with the id, or an empty string when no layer has it.</summary>
    public static string IDToName(int id) => IndexOf(id) is var index and >= 0 ? Table[index].Name : "";

    /// <summary>The layer's position relative to Default for an id, or 0 when no layer has the id.</summary>
    public static int GetLayerValueFromID(int id) => IndexOf(id) is var index and >= 0 ? index - IndexOf(0) : 0;

    /// <summary>The layer's position relative to Default for a name, or 0 when no layer has the name.</summary>
    public static int GetLayerValueFromName(string name) => TryFind(name, out var id) ? GetLayerValueFromID(id) : 0;

    /// <summary>Whether a layer has the id.</summary>
    public static bool IsValid(int id) => IndexOf(id) >= 0;

    /// <summary>
    /// The id for a name a script assigns to a renderer, group or canvas. A name no layer has leaves the layer
    /// as it is, and is logged once per name: the engine's choice, so one bad name does not stop a script.
    /// </summary>
    internal static bool TryResolve(string name, out int id)
    {
        if (TryFind(name, out id)) return true;
        if (Warned.Add(name)) Debug.LogWarning($"Sorting layer '{name}' is not registered; the layer is unchanged");
        return false;
    }

    /// <summary>
    /// Replaces the table with a player's layers in drawing order. Default (id 0) is added first when the list
    /// has no layer with id 0, so the Default layer always exists.
    /// </summary>
    internal static void SetLayers(IReadOnlyList<(string Name, int Id)> layers)
    {
        ArgumentNullException.ThrowIfNull(layers);
        var next = layers.ToList();
        if (next.Select(entry => entry.Id).Distinct().Count() != next.Count || next.Select(entry => entry.Name).Distinct().Count() != next.Count)
            throw new ArgumentException("sorting layer names and ids must each be unique", nameof(layers));
        if (!next.Any(entry => entry.Id == 0)) next.Insert(0, ("Default", 0));
        Table.Clear();
        Table.AddRange(next);
        Warned.Clear();
    }

    /// <summary>Restores the table of the Default layer alone.</summary>
    internal static void ClearLayers() => SetLayers([]);

    private static bool TryFind(string name, out int id)
    {
        ArgumentNullException.ThrowIfNull(name);
        var index = Table.FindIndex(entry => entry.Name == name);
        id = index >= 0 ? Table[index].Id : 0;
        return index >= 0;
    }

    private static int IndexOf(int id) => Table.FindIndex(entry => entry.Id == id);
}
