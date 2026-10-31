using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine.Rendering;

/// <summary>
/// Sorts the renderers of an object and its descendants as one unit under a layer and an order. The enabled state and
/// the settings are held by the native component, one per object, so a clone from <c>Instantiate</c> has its own.
/// <see cref="sortingOrder"/> is stored as given, like <see cref="Renderer.sortingOrder"/>. The engine registers no layer
/// but Default (id 0); another name or id throws <see cref="NotSupportedException"/>. Nothing draws yet, so the
/// settings only take effect once the render path reads them.
/// </summary>
public sealed class SortingGroup : Behaviour
{
    private protected override bool EnabledState
    {
        get => Read().enabled == 1;
        set { var settings = Read(); settings.enabled = value ? 1u : 0u; Write(settings); }
    }

    public int sortingLayerID
    {
        get => Read().sorting_layer;
        set { var settings = Read(); settings.sorting_layer = value; Write(settings); }
    }

    /// <summary>The name of the registered layer with <see cref="sortingLayerID"/>, or an empty string for an id no layer has.
    /// Setting a name no layer has keeps the current layer and logs a warning once per name.</summary>
    public string sortingLayerName
    {
        get => SortingLayer.IDToName(sortingLayerID);
        set { if (SortingLayer.TryResolve(value, out var id)) sortingLayerID = id; }
    }

    public int sortingOrder
    {
        get => Read().sorting_order;
        set { var settings = Read(); settings.sorting_order = value; Write(settings); }
    }

    /// <summary>Sorts the group against the other groups at the root of the hierarchy instead of its parent group.</summary>
    public bool sortAtRoot
    {
        get => Read().sort_at_root == 1;
        set { var settings = Read(); settings.sort_at_root = value ? 1u : 0u; Write(settings); }
    }

    /// <summary>Does nothing: the engine keeps no sorting state to refresh.</summary>
    public static void UpdateAllSortingGroups() { }

    private unsafe DisunitySortingGroup Read()
    {
        DisunitySortingGroup value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_sorting_group_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }

    private unsafe void Write(DisunitySortingGroup value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_sorting_group_write(DisunityHost.WorldHandle, Handle, &value));
}
