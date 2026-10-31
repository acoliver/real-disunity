using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>Names a user layer (8 to 31) for <see cref="LayerMask.NameToLayer"/> and <see cref="LayerMask.GetMask"/>,
    /// for a host whose plan carries no layer names.</summary>
    public static void SetUserLayerName(int layer, string name) => LayerMask.SetUserLayerName(layer, name);

    /// <summary>Replaces the names of all 32 layers with a project's, by layer index; a scene plan's
    /// <c>layers</c> come here.</summary>
    public static void SetLayerNames(IReadOnlyList<string> names) => LayerMask.SetLayerNames(names);

    /// <summary>Restores the built-in layer names and forgets every other.</summary>
    public static void ClearUserLayerNames() => LayerMask.ClearUserLayerNames();

    /// <summary>Replaces the sorting layer table with a player's layers, in drawing order; a scene plan's
    /// <c>sorting_layers</c> come here. Default (id 0) is added first when the list lacks it.</summary>
    public static void SetSortingLayers(IReadOnlyList<(string Name, int Id)> layers) => SortingLayer.SetLayers(layers);

    /// <summary>Restores the table of the Default sorting layer alone.</summary>
    public static void ClearSortingLayers() => SortingLayer.ClearLayers();
}
