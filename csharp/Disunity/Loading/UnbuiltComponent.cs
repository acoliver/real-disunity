using System.Collections.Concurrent;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>
/// A component a player plan could not build, as a script field's reference names it. The
/// field binds null, but only when the field's type could hold a component of that class.
/// </summary>
internal sealed record UnbuiltComponent(string Class)
{
    private static readonly ConcurrentDictionary<string, Type[]> Engine = new();

    /// <summary>True when a field of <paramref name="field"/> could hold a component of this class.
    /// Classes the engine does not define are held only by the field types every component
    /// satisfies and by a field of that class's own name.</summary>
    public bool CanBeHeldBy(Type field)
    {
        if (field.Name == Class || field == typeof(UnityEngine.Object) || field == typeof(Component)) return true;
        var known = Engine.GetOrAdd(Class, name => typeof(Component).Assembly.GetExportedTypes()
            .Where(t => t.Name == name && typeof(Component).IsAssignableFrom(t)).ToArray());
        return known.Any(field.IsAssignableFrom);
    }
}
