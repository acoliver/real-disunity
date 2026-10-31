using Disunity.Hosting;
using UnityEngine;

/// <summary>Reparenting under a parent scaled to zero.</summary>
internal static class SingularReparentTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            KeepsLocalValues();
            Console.WriteLine("PASS: SetParent under a parent scaled to zero keeps the child's local values instead of failing");
            LiveParentStillKeepsWorld();
            Console.WriteLine("PASS: SetParent under a scaled parent still keeps the world position");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void KeepsLocalValues()
    {
        var parent = DisunityHost.CreateGameObject();
        var child = DisunityHost.CreateGameObject();
        child.transform.localPosition = new Vector3(3, 4, 0);
        parent.transform.localScale = Vector3.zero;
        child.transform.SetParent(parent.transform);
        Expect.That(ReferenceEquals(child.transform.parent, parent.transform), "the child is under the parent");
        Expect.Equal(new Vector3(3, 4, 0), child.transform.localPosition, "local position is unchanged");
        var other = DisunityHost.CreateGameObject();
        other.transform.SetParent(parent.transform, true);
        Expect.That(ReferenceEquals(other.transform.parent, parent.transform), "SetParent(parent, true) also succeeds");
    }

    private static void LiveParentStillKeepsWorld()
    {
        var parent = DisunityHost.CreateGameObject();
        parent.transform.localScale = new Vector3(2, 2, 2);
        var child = DisunityHost.CreateGameObject();
        child.transform.position = new Vector3(4, 0, 0);
        child.transform.SetParent(parent.transform);
        Expect.Equal(new Vector3(2, 0, 0), child.transform.localPosition, "the local position compensates for the scale");
        Expect.Equal(new Vector3(4, 0, 0), child.transform.position, "the world position is kept");
    }
}
