using System.Collections;
using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>Children, sibling order, path lookup and the changed flag of a Transform. The native
/// world records the parent of each object; the order of siblings is kept here, in the order
/// objects were created or last moved.</summary>
public partial class Transform
{
    private static long _stamps;
    private long _changed = Interlocked.Increment(ref _stamps);
    private long _cleared;

    internal void MarkChanged() => _changed = Interlocked.Increment(ref _stamps);

    /// <summary>
    /// True after a script changes this transform's position, rotation, scale or parent, or any
    /// ancestor's, until a script sets it false. Moves the native world makes itself (physics) do
    /// not set it. A new transform reports true.
    /// </summary>
    public bool hasChanged
    {
        get
        {
            var latest = _changed;
            for (var ancestor = parent; ancestor is not null; ancestor = ancestor.parent) latest = Math.Max(latest, ancestor._changed);
            return latest > _cleared;
        }
        set
        {
            if (value) MarkChanged();
            else _cleared = Interlocked.Increment(ref _stamps);
        }
    }

    public void SetParent(Transform? p) => SetParent(p, true);

    public unsafe void SetParent(Transform? parent, bool worldPositionStays)
    {
        var target = parent?.gameObject.NativeHandle ?? 0;
        var moved = DisunityHost.ParentHandle(gameObject) != target;
        DisunityAdvance work;
        try { DisunityHost.ThrowIfError(NativeParent(target, worldPositionStays, &work)); }
        catch (DisunityException error) when (worldPositionStays && error.Reason == DisunityErrorReason.SingularParent)
        {
            // A parent scaled to zero has no inverse to keep the world pose through. The native world refuses and
            // changes nothing, so the engine's choice is to keep the local values, as worldPositionStays false does.
            DisunityHost.ThrowIfError(NativeParent(target, false, &work));
        }
        MarkChanged();
        if (moved) gameObject.SiblingKey = DisunityHost.NextSiblingKey();
        DisunityHost.Drain(work);
    }

    private unsafe DisunityResult NativeParent(ulong target, bool keepWorld, DisunityAdvance* work) =>
        NativeBindings.disunity_transform_parent(DisunityHost.WorldHandle, gameObject.NativeHandle, target, keepWorld ? 1u : 0u, work);

    public Transform root
    {
        get
        {
            var top = this;
            for (var next = top.parent; next is not null; next = top.parent) top = next;
            return top;
        }
    }

    private List<GameObject> Siblings() => DisunityHost.ChildrenOf(DisunityHost.ParentHandle(gameObject));

    public int childCount => DisunityHost.ChildrenOf(gameObject.NativeHandle).Count;

    [Obsolete("Use Transform.childCount instead.")]
    public int GetChildCount() => childCount;

    /// <summary>The child at <paramref name="index"/> in sibling order.</summary>
    public Transform GetChild(int index)
    {
        var children = DisunityHost.ChildrenOf(gameObject.NativeHandle);
        if ((uint)index >= (uint)children.Count) throw new ArgumentOutOfRangeException(nameof(index), "Transform child out of bounds");
        return children[index].transform;
    }

    public IEnumerator GetEnumerator() => DisunityHost.ChildrenOf(gameObject.NativeHandle).Select(child => child.transform).GetEnumerator();

    public int GetSiblingIndex() => Siblings().IndexOf(gameObject);

    /// <summary>Moves this transform among its siblings. A negative index counts from the end (-1 is last); an index
    /// past either end moves it to that end.</summary>
    public void SetSiblingIndex(int index)
    {
        var siblings = Siblings();
        siblings.Remove(gameObject);
        var total = siblings.Count + 1;
        var position = Math.Clamp(index < 0 ? total + index : index, 0, total - 1);
        siblings.Insert(position, gameObject);
        foreach (var sibling in siblings) sibling.SiblingKey = DisunityHost.NextSiblingKey();
    }

    public void SetAsFirstSibling() => SetSiblingIndex(0);
    public void SetAsLastSibling() => SetSiblingIndex(-1);

    /// <summary>Makes every child a root, keeping its world position.</summary>
    public void DetachChildren()
    {
        foreach (var child in DisunityHost.ChildrenOf(gameObject.NativeHandle)) child.transform.SetParent(null, true);
    }

    /// <summary>The descendant at a '/'-separated path of names below this transform, or null.
    /// Inactive objects are found; the first child with a name wins.</summary>
    public Transform? Find(string n)
    {
        ArgumentNullException.ThrowIfNull(n);
        Transform? current = this;
        foreach (var name in n.Split('/'))
        {
            current = DisunityHost.ChildrenOf(current.gameObject.NativeHandle).FirstOrDefault(child => child.name == name)?.transform;
            if (current is null) return null;
        }
        return current;
    }

    [Obsolete("FindChild has been renamed to Find.")]
    public Transform? FindChild(string n) => Find(n);

    /// <summary>True for this transform and every ancestor of it.</summary>
    public bool IsChildOf(Transform? parent)
    {
        if (parent is null) return false;
        for (Transform? current = this; current is not null; current = current.parent)
            if (ReferenceEquals(current.gameObject, parent.gameObject)) return true;
        return false;
    }
}
