using UnityEngine;

namespace Disunity.Hosting;

/// <summary>The execution order of each script class. A player scene's plan carries the order the player stores
/// with a script, 0 included, and that order replaces the class's <c>DefaultExecutionOrder</c>. A source-project
/// plan carries none, and the attribute applies. The order applies to every instance of the class, however it
/// is created.</summary>
internal static class ScriptExecutionOrders
{
    private static readonly Dictionary<Type, int> Planned = new();

    internal static void Reset() => Planned.Clear();

    /// <summary>Sets the order of <paramref name="type"/> to the one its scene plan carries.</summary>
    internal static void Plan(Type type, int order) => Planned[type] = order;

    /// <summary>The order of the type: the plan's, even when it is 0, else the attribute's on the type or a base type, else 0.</summary>
    internal static int Of(Type type) =>
        Planned.TryGetValue(type, out var planned) ? planned
            : Attribute.GetCustomAttribute(type, typeof(DefaultExecutionOrder), inherit: true) is DefaultExecutionOrder attribute ? attribute.order : 0;
}
