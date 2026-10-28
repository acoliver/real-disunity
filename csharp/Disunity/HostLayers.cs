using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>Names a user layer (8 to 31) for <see cref="LayerMask.NameToLayer"/> and <see cref="LayerMask.GetMask"/>.
    /// A player plan does not carry its project's layer names yet, so the host supplies them.</summary>
    public static void SetUserLayerName(int layer, string name) => LayerMask.SetUserLayerName(layer, name);

    /// <summary>Forgets every user layer name.</summary>
    public static void ClearUserLayerNames() => LayerMask.ClearUserLayerNames();
}
