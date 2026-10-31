using Disunity.Hosting;

namespace UnityEngine;

/// <summary>The searches behind the GetComponent family of <see cref="GameObject"/> and <see cref="Component"/>.
/// Components come in attachment order, the transform first. A search below or above an object includes
/// inactive objects only when asked, as Unity's does.</summary>
internal static class ComponentQueries
{
    public static IEnumerable<Component> On(GameObject owner, Type type)
    {
        ArgumentNullException.ThrowIfNull(type);
        return DisunityHost.Components(owner).Distinct().Where(type.IsInstanceOfType);
    }

    /// <summary>The object and then its descendants, depth first in sibling order.</summary>
    public static IEnumerable<GameObject> Descendants(GameObject root)
    {
        var pending = new Stack<GameObject>();
        pending.Push(root);
        while (pending.Count > 0)
        {
            var next = pending.Pop();
            yield return next;
            var below = DisunityHost.ChildrenOf(next.RawHandle);
            for (var i = below.Count - 1; i >= 0; i--) pending.Push(below[i]);
        }
    }

    /// <summary>The object, its parent, and so on up to the root.</summary>
    public static IEnumerable<GameObject> Ancestors(GameObject start)
    {
        for (Transform? current = start.transform; current is not null; current = current.parent) yield return current.gameObject;
    }

    public static IEnumerable<Component> InChildren(GameObject root, Type type, bool includeInactive) =>
        Descendants(root).Where(value => includeInactive || value.activeInHierarchy).SelectMany(value => On(value, type));

    public static IEnumerable<Component> InParents(GameObject start, Type type, bool includeInactive) =>
        Ancestors(start).Where(value => includeInactive || value.activeInHierarchy).SelectMany(value => On(value, type));

    /// <summary>Replaces the contents of <paramref name="results"/>, as Unity's list overloads do.</summary>
    public static void Fill<T>(List<T> results, IEnumerable<Component> found)
    {
        ArgumentNullException.ThrowIfNull(results);
        results.Clear();
        results.AddRange(found.Cast<T>());
    }
}
