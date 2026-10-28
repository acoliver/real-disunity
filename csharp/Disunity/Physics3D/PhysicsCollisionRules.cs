namespace UnityEngine;

public partial class Physics
{
    private const int LayerCount = 32;

    // One bit per layer pair, kept symmetric: bit b of row a is set when layer a and layer b ignore each other.
    private static readonly uint[] IgnoredLayers = new uint[LayerCount];
    private static readonly HashSet<(int, int)> IgnoredColliders = [];

    public static void IgnoreLayerCollision(int layer1, int layer2, bool ignore)
    {
        CheckLayer(layer1, nameof(layer1));
        CheckLayer(layer2, nameof(layer2));
        Set(layer1, layer2, ignore);
        Set(layer2, layer1, ignore);
    }

    public static void IgnoreLayerCollision(int layer1, int layer2) => IgnoreLayerCollision(layer1, layer2, true);

    public static bool GetIgnoreLayerCollision(int layer1, int layer2)
    {
        CheckLayer(layer1, nameof(layer1));
        CheckLayer(layer2, nameof(layer2));
        return (IgnoredLayers[layer1] & (1u << layer2)) != 0;
    }

    public static void IgnoreCollision(Collider collider1, Collider collider2, bool ignore)
    {
        var pair = PairOf(collider1, collider2);
        if (ignore) IgnoredColliders.Add(pair);
        else IgnoredColliders.Remove(pair);
    }

    public static void IgnoreCollision(Collider collider1, Collider collider2) => IgnoreCollision(collider1, collider2, true);

    public static bool GetIgnoreCollision(Collider collider1, Collider collider2) =>
        IgnoredColliders.Contains(PairOf(collider1, collider2));

    private static void Set(int row, int column, bool ignore)
    {
        if (ignore) IgnoredLayers[row] |= 1u << column;
        else IgnoredLayers[row] &= ~(1u << column);
    }

    private static void CheckLayer(int layer, string name)
    {
        if (layer is < 0 or >= LayerCount) throw new ArgumentOutOfRangeException(name, layer, "A layer is 0 to 31");
    }

    private static (int, int) PairOf(Collider collider1, Collider collider2)
    {
        ArgumentNullException.ThrowIfNull(collider1);
        ArgumentNullException.ThrowIfNull(collider2);
        var first = collider1.GetInstanceID();
        var second = collider2.GetInstanceID();
        return first < second ? (first, second) : (second, first);
    }
}
