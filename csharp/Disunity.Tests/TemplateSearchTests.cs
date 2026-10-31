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
            SceneSearchesStillSkipInactiveParents();
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

    private static void SceneSearchesStillSkipInactiveParents()
    {
        var root = new GameObject("scene search root");
        var child = new GameObject("scene search child");
        child.transform.SetParent(root.transform);
        var filter = child.AddComponent<MeshFilter>();
        Expect.That(ReferenceEquals(root.GetComponentInChildren<MeshFilter>(), filter), "an active scene child is found");
        root.SetActive(false);
        Expect.That(root.GetComponentInChildren<MeshFilter>() == null && child.GetComponentInChildren<MeshFilter>() == null, "an inactive scene parent hides its children");
        Expect.That(ReferenceEquals(root.GetComponentInChildren<MeshFilter>(true), filter), "including inactive still finds it");
        UnityEngine.Object.Destroy(root);
        Console.WriteLine("PASS: scene searches still skip objects under an inactive parent");
    }
}
