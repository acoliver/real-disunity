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

    // Searches count an object as active when it and every ancestor are active themselves. For a scene object
    // that is activeInHierarchy; it also holds for prefab templates and objects still under construction, which
    // are not in the scene yet, so a search on a loaded prefab finds its active children (the engine's choice).
    public static IEnumerable<Component> InChildren(GameObject root, Type type, bool includeInactive) =>
        (includeInactive ? Descendants(root) : ActiveDescendants(root)).SelectMany(value => On(value, type));

    public static IEnumerable<Component> InParents(GameObject start, Type type, bool includeInactive) =>
        (includeInactive ? Ancestors(start) : ActiveAncestors(start)).SelectMany(value => On(value, type));

    /// <summary>The descendants a search finds, depth first in sibling order, skipping each inactive subtree.</summary>
    private static IEnumerable<GameObject> ActiveDescendants(GameObject root)
    {
        if (!Ancestors(root).All(value => value.activeSelf)) yield break;
        var pending = new Stack<GameObject>();
        pending.Push(root);
        while (pending.Count > 0)
        {
            var next = pending.Pop();
            yield return next;
            var children = DisunityHost.ChildrenOf(next.RawHandle);
            for (var i = children.Count - 1; i >= 0; i--)
                if (children[i].activeSelf) pending.Push(children[i]);
        }
    }

    /// <summary>The ancestors a search finds: those whose own chain up to the root is active.</summary>
    private static IEnumerable<GameObject> ActiveAncestors(GameObject start)
    {
        var chain = Ancestors(start).ToList();
        var firstActive = chain.FindLastIndex(value => !value.activeSelf) + 1;
        return chain.Skip(firstActive);
    }

    /// <summary>Replaces the contents of <paramref name="results"/>, as Unity's list overloads do.</summary>
    public static void Fill<T>(List<T> results, IEnumerable<Component> found)
    {
        ArgumentNullException.ThrowIfNull(results);
        results.Clear();
        results.AddRange(found.Cast<T>());
    }
}
