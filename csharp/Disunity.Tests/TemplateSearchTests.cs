using Disunity.Hosting;
using UnityEngine;

internal static class TemplateSearchTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            TemplateSearchesFindActiveChildren();
            SceneSearchesStartAtTheCallerNotAtTheRoot();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void TemplateSearchesFindActiveChildren()
    {
        using var templates = DisunityHost.BeginConstruction();
        var root = templates.CreateGameObject();
        var shown = templates.CreateGameObject();
        var hidden = templates.CreateGameObject();
        var below = templates.CreateGameObject();
        shown.transform.SetParent(root.transform);
        hidden.transform.SetParent(root.transform);
        below.transform.SetParent(hidden.transform);
        var rootGroup = root.AddComponent<CanvasGroup>();
        var shownFilter = shown.AddComponent<MeshFilter>();
        hidden.AddComponent<MeshFilter>();
        below.AddComponent<MeshFilter>();
        hidden.SetActive(false);
        Expect.That(!root.activeInHierarchy, "a template is not in the scene");
        Expect.That(ReferenceEquals(root.GetComponentInChildren<MeshFilter>(), shownFilter), "a template search finds its active child");
        Expect.Equal(1, root.GetComponentsInChildren<MeshFilter>().Length, "an inactive template child and its subtree are skipped");
        Expect.Equal(3, root.GetComponentsInChildren<MeshFilter>(true).Length, "including inactive finds every filter");
        Expect.That(ReferenceEquals(shown.GetComponentInParent<CanvasGroup>(), rootGroup), "a template child finds its parent's component");
        Expect.That(ReferenceEquals(below.GetComponentInParent<CanvasGroup>(), rootGroup), "an object under an inactive template child still finds the active root's component");
        Expect.That(below.GetComponentInParent<MeshFilter>() == null, "the inactive ancestors' filters are skipped");
        Expect.Equal(2, below.GetComponentsInParent<MeshFilter>(true).Length, "including inactive finds the filters up the chain");
        Expect.That(hidden.GetComponentInChildren<MeshFilter>() == null, "a search from an inactive template object finds nothing");
        Console.WriteLine("PASS: searches on a template count objects whose chain is active themselves");
    }

    private static void SceneSearchesStartAtTheCallerNotAtTheRoot()
    {
        var root = new GameObject("scene search root");
        var child = new GameObject("scene search child");
        var grandchild = new GameObject("scene search grandchild");
        child.transform.SetParent(root.transform);
        grandchild.transform.SetParent(child.transform);
        var filter = child.AddComponent<MeshFilter>();
        var deep = grandchild.AddComponent<MeshFilter>();
        Expect.That(ReferenceEquals(root.GetComponentInChildren<MeshFilter>(), filter), "an active scene child is found");
        root.SetActive(false);
        Expect.That(root.GetComponentInChildren<MeshFilter>() == null, "an inactive object finds nothing below it");
        Expect.That(ReferenceEquals(child.GetComponentInChildren<MeshFilter>(), filter), "an active object under an inactive parent still finds its own components (the engine's choice, taken from the original game's run)");
        Expect.Equal(2, child.GetComponentsInChildren<MeshFilter>().Length, "and its active descendants");
        grandchild.SetActive(false);
        Expect.That(ReferenceEquals(child.GetComponentInChildren<MeshFilter>(), filter) && child.GetComponentsInChildren<MeshFilter>().Length == 1,
            "an inactive descendant and its subtree are still skipped");
        Expect.That(ReferenceEquals(root.GetComponentInChildren<MeshFilter>(true), filter), "including inactive still finds it");
        Expect.That(ReferenceEquals(grandchild.GetComponentInChildren<MeshFilter>(true), deep), "including inactive finds an inactive object's own component");
        child.SetActive(false);
        Expect.That(child.GetComponentInChildren<MeshFilter>() == null, "an object that is inactive itself finds nothing");
        UnityEngine.Object.Destroy(root);
        Console.WriteLine("PASS: a scene search from an object under an inactive parent finds the object's active subtree");
    }
}
