using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Transform axes, space conversion, relative moves, sibling order, path lookup and the changed flag, with hand-computed values.</summary>
internal static class TransformMemberTests
{
    private static readonly Vector3 Position = new(1, 2, 3);

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Axes();
            SpaceConversion();
            RelativeMoves();
            Turning();
            Parenting();
            SiblingOrder();
            PathsAndAncestry();
            HasChanged();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: Transform axes, Euler angles, space conversion, Translate, Rotate, RotateAround, LookAt, SetParent, sibling order, Find, IsChildOf and hasChanged");
    }

    private static void Close(Vector3 expected, Vector3 actual, string what) =>
        Ensure((expected - actual).magnitude < 1e-4f, $"{what}: expected {expected}, got {actual}");

    /// <summary>At (1,2,3), turned 90 degrees about Y and scaled by 2.</summary>
    private static Transform Posed()
    {
        var transform = DisunityHost.CreateGameObject().transform;
        transform.localPosition = Position;
        transform.localRotation = Quaternion.Euler(0, 90, 0);
        transform.localScale = new Vector3(2, 2, 2);
        return transform;
    }

    private static void Axes()
    {
        var transform = Posed();
        Close(new Vector3(1, 0, 0), transform.forward, "a quarter turn about Y sends forward to +X");
        Close(new Vector3(0, 0, -1), transform.right, "and right to -Z");
        Close(new Vector3(0, 1, 0), transform.up, "and leaves up");
        Close(new Vector3(0, 90, 0), transform.eulerAngles, "eulerAngles");
        Close(new Vector3(0, 90, 0), transform.localEulerAngles, "localEulerAngles");
        transform.eulerAngles = new Vector3(0, 180, 0);
        Close(new Vector3(0, 0, -1), transform.forward, "setting eulerAngles turns the object");
        transform.localEulerAngles = new Vector3(90, 0, 0);
        Close(new Vector3(0, -1, 0), transform.forward, "setting localEulerAngles turns the object");
        transform.forward = new Vector3(1, 0, 0);
        Close(new Vector3(1, 0, 0), transform.forward, "forward can be set");
    }

    private static void SpaceConversion()
    {
        var transform = Posed();
        Close(new Vector3(3, 2, 3), transform.TransformPoint(new Vector3(0, 0, 1)), "TransformPoint applies scale, rotation and position");
        Close(new Vector3(3, 2, 3), transform.TransformPoint(0, 0, 1), "TransformPoint(x, y, z)");
        Close(new Vector3(0, 0, 1), transform.InverseTransformPoint(new Vector3(3, 2, 3)), "InverseTransformPoint undoes it");
        Close(new Vector3(1, 0, 0), transform.TransformDirection(new Vector3(0, 0, 1)), "TransformDirection ignores scale");
        Close(new Vector3(0, 0, 1), transform.InverseTransformDirection(new Vector3(1, 0, 0)), "InverseTransformDirection");
        Close(new Vector3(2, 0, 0), transform.TransformVector(new Vector3(0, 0, 1)), "TransformVector applies scale but not position");
        Close(new Vector3(0, 0, 0.5f), transform.InverseTransformVector(new Vector3(1, 0, 0)), "InverseTransformVector");
        Close(new Vector3(2, 2, 2), transform.lossyScale, "lossyScale");
        Close(new Vector3(3, 2, 3), transform.localToWorldMatrix.MultiplyPoint3x4(new Vector3(0, 0, 1)), "localToWorldMatrix");
        Close(new Vector3(0, 0, 1), transform.worldToLocalMatrix.MultiplyPoint3x4(new Vector3(3, 2, 3)), "worldToLocalMatrix");

        var child = DisunityHost.CreateGameObject().transform;
        child.SetParent(transform, false);
        child.localPosition = new Vector3(0, 0, 1);
        child.localScale = new Vector3(1, 3, 1);
        Close(new Vector3(3, 2, 3), child.position, "a child's world position goes through the parent");
        Close(new Vector3(2, 6, 2), child.lossyScale, "lossyScale multiplies the chain");
        Close(new Vector3(5, 2, 3), child.TransformPoint(new Vector3(0, 0, 1)), "the chain: local (0,0,1) is world (5,2,3)");
        Close(new Vector3(0, 0, 1), child.InverseTransformPoint(new Vector3(5, 2, 3)), "and back");
    }

    private static void RelativeMoves()
    {
        var transform = DisunityHost.CreateGameObject().transform;
        transform.position = Position;
        transform.rotation = Quaternion.Euler(0, 90, 0);
        transform.Translate(new Vector3(0, 0, 1));
        Close(new Vector3(2, 2, 3), transform.position, "Translate defaults to Space.Self, here along +X");
        transform.Translate(new Vector3(0, 0, 1), Space.World);
        Close(new Vector3(2, 2, 4), transform.position, "Space.World moves along the world axes");
        transform.Translate(0, 1, 0);
        Close(new Vector3(2, 3, 4), transform.position, "Translate(x, y, z) is Self");
        var other = DisunityHost.CreateGameObject().transform;
        other.rotation = Quaternion.Euler(0, 180, 0);
        transform.Translate(new Vector3(0, 0, 1), other);
        Close(new Vector3(2, 3, 3), transform.position, "relative to another transform follows its axes");
        transform.Translate(new Vector3(1, 0, 0), (Transform?)null);
        Close(new Vector3(3, 3, 3), transform.position, "a null transform means the world axes");

        var scaled = DisunityHost.CreateGameObject().transform;
        scaled.localScale = new Vector3(4, 4, 4);
        scaled.Translate(new Vector3(0, 0, 1));
        Close(new Vector3(0, 0, 1), scaled.position, "Self translation is not scaled");

        transform.SetPositionAndRotation(new Vector3(7, 8, 9), Quaternion.Euler(0, 0, 90));
        Close(new Vector3(7, 8, 9), transform.position, "SetPositionAndRotation position");
        Close(new Vector3(0, 0, 90), transform.eulerAngles, "SetPositionAndRotation rotation");
        transform.GetPositionAndRotation(out var position, out var rotation);
        Close(new Vector3(7, 8, 9), position, "GetPositionAndRotation position");
        Ensure(rotation == Quaternion.Euler(0, 0, 90), "GetPositionAndRotation rotation");
    }

    private static void Turning()
    {
        var transform = DisunityHost.CreateGameObject().transform;
        transform.rotation = Quaternion.Euler(0, 90, 0);
        transform.Rotate(new Vector3(90, 0, 0), Space.World);
        Close(new Vector3(1, 0, 0), transform.forward, "a world X turn leaves a +X forward alone");
        transform.rotation = Quaternion.Euler(0, 90, 0);
        transform.Rotate(new Vector3(90, 0, 0), Space.Self);
        Close(new Vector3(0, -1, 0), transform.forward, "a self X turn tips forward down");
        transform.rotation = Quaternion.identity;
        transform.Rotate(new Vector3(0, 90, 0));
        Close(new Vector3(1, 0, 0), transform.forward, "Rotate(eulers) is Self");
        transform.rotation = Quaternion.identity;
        transform.Rotate(0, 90, 0);
        Close(new Vector3(1, 0, 0), transform.forward, "Rotate(x, y, z)");
        transform.rotation = Quaternion.identity;
        transform.Rotate(Vector3.up, 90);
        Close(new Vector3(1, 0, 0), transform.forward, "Rotate(axis, angle) is Self");
        transform.rotation = Quaternion.Euler(0, 90, 0);
        transform.Rotate(Vector3.right, 90, Space.World);
        Close(new Vector3(1, 0, 0), transform.forward, "Rotate(axis, angle, World)");

        transform.position = new Vector3(2, 0, 0);
        transform.rotation = Quaternion.identity;
        transform.RotateAround(Vector3.zero, Vector3.up, 90);
        Close(new Vector3(0, 0, -2), transform.position, "RotateAround orbits the point");
        Close(new Vector3(1, 0, 0), transform.forward, "RotateAround turns the object about the same axis");
        transform.position = new Vector3(3, 0, 0);
        transform.RotateAround(new Vector3(1, 0, 0), Vector3.up, 90);
        Close(new Vector3(1, 0, -2), transform.position, "RotateAround keeps the offset to an off-origin point");

        transform.position = Vector3.zero;
        transform.LookAt(new Vector3(5, 0, 0));
        Close(new Vector3(1, 0, 0), transform.forward, "LookAt a position");
        var target = DisunityHost.CreateGameObject().transform;
        target.position = new Vector3(0, 0, 5);
        transform.LookAt(target);
        Close(new Vector3(0, 0, 1), transform.forward, "LookAt a transform");
        transform.LookAt(new Vector3(0, 5, 5), Vector3.right);
        Close(new Vector3(0, 1, 1).normalized, transform.forward, "LookAt aims at the point");
        var before = transform.rotation;
        transform.LookAt(transform.position);
        Ensure(transform.rotation == before, "LookAt its own position changes nothing");
    }

    private static void Parenting()
    {
        var parent = DisunityHost.CreateGameObject().transform;
        parent.position = new Vector3(1, 0, 0);
        var kept = DisunityHost.CreateGameObject().transform;
        kept.position = new Vector3(5, 0, 0);
        kept.SetParent(parent);
        Close(new Vector3(5, 0, 0), kept.position, "SetParent keeps the world position");
        Close(new Vector3(4, 0, 0), kept.localPosition, "so the local position changes");
        var moved = DisunityHost.CreateGameObject().transform;
        moved.position = new Vector3(5, 0, 0);
        moved.SetParent(parent, false);
        Close(new Vector3(6, 0, 0), moved.position, "worldPositionStays false keeps the local position");
        Ensure(ReferenceEquals(moved.parent, parent) && ReferenceEquals(moved.root, parent), "parent and root");
        moved.SetParent(null);
        Ensure(moved.parent is null && ReferenceEquals(moved.root, moved), "SetParent(null) makes a root");
    }

    private static Transform Child(Transform parent, string name)
    {
        var child = DisunityHost.CreateGameObject();
        child.name = name;
        child.transform.SetParent(parent, false);
        return child.transform;
    }

    private static string Order(Transform parent) =>
        string.Concat(Enumerable.Range(0, parent.childCount).Select(i => parent.GetChild(i).name));

    private static void SiblingOrder()
    {
        var parent = DisunityHost.CreateGameObject().transform;
        var (a, b, c) = (Child(parent, "a"), Child(parent, "b"), Child(parent, "c"));
        Ensure(parent.childCount == 3 && Order(parent) == "abc", "children come in the order they were added");
        Ensure(a.GetSiblingIndex() == 0 && b.GetSiblingIndex() == 1 && c.GetSiblingIndex() == 2, "GetSiblingIndex");
        c.SetAsFirstSibling();
        Ensure(Order(parent) == "cab", "SetAsFirstSibling");
        c.SetAsLastSibling();
        Ensure(Order(parent) == "abc", "SetAsLastSibling");
        a.SetSiblingIndex(1);
        Ensure(Order(parent) == "bac", "SetSiblingIndex moves among the siblings");
        b.SetSiblingIndex(-1);
        Ensure(Order(parent) == "acb", "a negative index counts from the end");
        c.SetSiblingIndex(99);
        Ensure(Order(parent) == "abc", "an index past the end means last");
        Ensure(string.Concat(parent.Cast<Transform>().Select(t => t.name)) == "abc", "foreach visits the children in order");
        Throws<ArgumentOutOfRangeException>(() => parent.GetChild(3), "GetChild out of range");
        Throws<ArgumentOutOfRangeException>(() => parent.GetChild(-1), "GetChild negative");

        var other = DisunityHost.CreateGameObject().transform;
        a.SetParent(other, false);
        Ensure(Order(parent) == "bc" && Order(other) == "a", "SetParent moves a child between parents");
        b.SetParent(other, false);
        Ensure(Order(other) == "ab", "a reparented child becomes the last sibling");
        b.SetParent(other, false);
        a.SetAsLastSibling();
        b.SetParent(other, false);
        Ensure(Order(other) == "ba", "setting the same parent keeps the sibling order");

        var roots = DisunityHost.CreateGameObject().transform;
        Ensure(roots.GetSiblingIndex() >= 0, "a root has a sibling index among the roots");
        parent.DetachChildren();
        Ensure(parent.childCount == 0 && c.parent is null, "DetachChildren makes the children roots");
    }

    private static void PathsAndAncestry()
    {
        var top = DisunityHost.CreateGameObject().transform;
        var mid = Child(top, "mid");
        var leaf = Child(mid, "leaf");
        var hidden = Child(mid, "hidden");
        hidden.gameObject.SetActive(false);
        Ensure(ReferenceEquals(top.Find("mid"), mid), "Find a child");
        Ensure(ReferenceEquals(top.Find("mid/leaf"), leaf), "Find follows a path");
        Ensure(ReferenceEquals(top.Find("mid/hidden"), hidden), "Find also finds inactive objects");
        Ensure(top.Find("leaf") is null && top.Find("mid/none") is null && top.Find("") is null, "Find returns null when there is no such path");
        Throws<ArgumentNullException>(() => top.Find(null!), "Find(null)");
        Ensure(leaf.IsChildOf(mid) && leaf.IsChildOf(top) && leaf.IsChildOf(leaf), "IsChildOf is true for ancestors and itself");
        Ensure(!top.IsChildOf(leaf) && !leaf.IsChildOf(null), "and false otherwise");
        Ensure(!leaf.IsChildOf(Child(top, "sibling")), "a sibling is not an ancestor");
    }

    private static void HasChanged()
    {
        var parent = DisunityHost.CreateGameObject().transform;
        var child = Child(parent, "c");
        Ensure(parent.hasChanged && child.hasChanged, "a new transform reports a change");
        parent.hasChanged = false;
        child.hasChanged = false;
        Ensure(!parent.hasChanged && !child.hasChanged, "a script can clear the flag");
        child.localScale = new Vector3(2, 2, 2);
        Ensure(child.hasChanged && !parent.hasChanged, "scaling a child marks only the child");
        child.hasChanged = false;
        parent.position = new Vector3(1, 1, 1);
        Ensure(parent.hasChanged && child.hasChanged, "moving a parent marks its children too");
        parent.hasChanged = false;
        child.hasChanged = false;
        child.rotation = Quaternion.Euler(0, 10, 0);
        Ensure(child.hasChanged, "rotating marks the transform");
        child.hasChanged = false;
        child.SetParent(null, true);
        Ensure(child.hasChanged, "a parent change marks the transform");
        child.hasChanged = false;
        child.hasChanged = true;
        Ensure(child.hasChanged, "a script can set the flag");
    }
}
