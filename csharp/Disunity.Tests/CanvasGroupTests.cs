using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;
using EngineObject = UnityEngine.Object;

/// <summary>The native CanvasGroup component: defaults, setters, alpha range, enabled, its place in the
/// component hierarchy, cloning, the loader and the raycast filter.</summary>
internal static class CanvasGroupTests
{
    private const string GroupJson = """
        {"type":"CanvasGroup","enabled":false,"alpha":0.25,"interactable":false,"blocks_raycasts":false,"ignore_parent_groups":true}
        """;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            Console.WriteLine("PASS: CanvasGroup defaults, setters and enabled");
            AlphaRange();
            Console.WriteLine("PASS: CanvasGroup alpha clamps to 0 through 1 and rejects NaN");
            Hierarchy();
            Console.WriteLine("PASS: a CanvasGroup is a Behaviour that GetComponents<MonoBehaviour> does not return");
            CloneIsolation();
            Console.WriteLine("PASS: a cloned CanvasGroup has its own native settings");
            Loaded();
            Console.WriteLine("PASS: a player plan builds a CanvasGroup with every serialized setting");
            Raycast();
            Console.WriteLine("PASS: CanvasGroup.IsRaycastLocationValid follows blocksRaycasts");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static CanvasGroup NewGroup() => DisunityHost.CreateGameObject().AddComponent<CanvasGroup>();

    private static void Defaults()
    {
        var group = NewGroup();
        Ensure(group.enabled && group.alpha == 1f && group.interactable && group.blocksRaycasts && !group.ignoreParentGroups, "engine defaults");
        Ensure(group.gameObject.GetComponent<CanvasGroup>() == group, "GetComponent finds the group");
        group.alpha = 0.5f;
        group.interactable = false;
        group.blocksRaycasts = false;
        group.ignoreParentGroups = true;
        Ensure(group.alpha == 0.5f && !group.interactable && !group.blocksRaycasts && group.ignoreParentGroups && group.enabled, "each setter changes only its own setting");
        group.interactable = true;
        Ensure(group.interactable && !group.blocksRaycasts && group.alpha == 0.5f, "setting one flag leaves the others");
        group.enabled = false;
        Ensure(!group.enabled && group.alpha == 0.5f && group.ignoreParentGroups, "a disabled group keeps its settings");
        group.enabled = true;
        Ensure(group.enabled, "enabled turns back on");
        var owner = group.gameObject;
        EngineObject.Destroy(owner);
        DisunityHost.Step(0.01f);
        Ensure(!group, "destroying the object destroys the group");
    }

    private static void AlphaRange()
    {
        var group = NewGroup();
        group.alpha = 2f;
        Ensure(group.alpha == 1f, "alpha clamps down to 1");
        group.alpha = -1f;
        Ensure(group.alpha == 0f, "alpha clamps up to 0");
        group.alpha = float.PositiveInfinity;
        Ensure(group.alpha == 1f, "infinite alpha clamps to 1");
        group.alpha = 0.75f;
        Throws<DisunityException>(() => group.alpha = float.NaN, "NaN alpha");
        Ensure(group.alpha == 0.75f, "a rejected value leaves alpha unchanged");
    }

    private static void Hierarchy()
    {
        var group = NewGroup();
        Ensure(group is Behaviour && !typeof(MonoBehaviour).IsAssignableFrom(typeof(CanvasGroup)), "a CanvasGroup is a Behaviour and not a MonoBehaviour");
        var owner = group.gameObject;
        var script = owner.AddComponent<GroupScriptProbe>();
        Ensure(owner.GetComponents<Behaviour>().Contains(group), "GetComponents<Behaviour> includes the group");
        var scripts = owner.GetComponents<MonoBehaviour>();
        Ensure(scripts.Length == 1 && scripts[0] == script, "GetComponents<MonoBehaviour> returns only the script");
        Ensure(EngineObject.FindObjectsByType<CanvasGroup>(FindObjectsSortMode.None).Contains(group), "FindObjectsByType<CanvasGroup> finds the group");
        Throws<DisunityException>(() => owner.AddComponent<CanvasGroup>(), "a second group on one object", "duplicate");
        EngineObject.Destroy(owner);
        DisunityHost.Step(0.01f);
    }

    private static void CloneIsolation()
    {
        var original = NewGroup();
        original.alpha = 0.5f;
        original.interactable = false;
        original.enabled = false;
        var clone = EngineObject.Instantiate(original);
        Ensure(clone != original && clone.gameObject != original.gameObject, "the clone is a separate group on a separate object");
        Ensure(clone.alpha == 0.5f && !clone.interactable && !clone.enabled && clone.blocksRaycasts, "the clone starts with the original's settings");
        clone.alpha = 0.1f;
        clone.enabled = true;
        Ensure(original.alpha == 0.5f && !original.enabled, "changing the clone leaves the original's settings");
        original.blocksRaycasts = false;
        Ensure(clone.blocksRaycasts, "changing the original leaves the clone's settings");
        EngineObject.Destroy(clone.gameObject);
        DisunityHost.Step(0.01f);
        Ensure(!clone && original && original.alpha == 0.5f, "destroying the clone leaves the original");
    }

    private static void Loaded()
    {
        using var loaded = PlayerCameraPlanTests.Load(GroupJson);
        var group = loaded.Objects[0].GetComponent<CanvasGroup>()!;
        Ensure(group.alpha == 0.25f && !group.interactable && !group.blocksRaycasts && group.ignoreParentGroups, "alpha and flags");
        Ensure(!group.enabled, "the plan's enabled flag");
    }

    private static void Raycast()
    {
        var group = NewGroup();
        ICanvasRaycastFilter filter = group;
        Ensure(filter.IsRaycastLocationValid(new Vector2(3f, 4f), null!), "a group that blocks raycasts accepts a location");
        group.blocksRaycasts = false;
        Ensure(!filter.IsRaycastLocationValid(new Vector2(3f, 4f), null!), "a group that does not block raycasts rejects it");
        group.interactable = false;
        group.blocksRaycasts = true;
        Ensure(filter.IsRaycastLocationValid(Vector2.zero, null!), "interactable does not change the raycast answer");
    }

    private sealed class GroupScriptProbe : MonoBehaviour { }
}
