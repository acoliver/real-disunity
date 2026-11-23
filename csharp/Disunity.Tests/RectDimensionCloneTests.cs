using Disunity.Hosting;
using UnityEngine;

/// <summary>A script on an instantiated object hears OnRectTransformDimensionsChange like one added with AddComponent:
/// layout groups, text and graphics of an instantiated prefab lay out again when the parent they are put under resizes.</summary>
internal static class RectDimensionCloneTests
{
    private sealed class Watcher : MonoBehaviour
    {
        public readonly List<Vector2> Sizes = new();
        private void OnRectTransformDimensionsChange() => Sizes.Add(((RectTransform)transform).rect.size);
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.SetViewport(800, 600);
            var canvas = NewRect(null);
            canvas.gameObject.AddComponent<Canvas>().renderMode = RenderMode.ScreenSpaceOverlay;
            InstantiatedScriptsHearTheirRectangle(Stretched(canvas));
            Console.WriteLine("PASS: OnRectTransformDimensionsChange reaches the scripts of an instantiated object");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void InstantiatedScriptsHearTheirRectangle(RectTransform holder)
    {
        var template = Stretched(holder);
        template.gameObject.AddComponent<Watcher>();
        Stretched(template).gameObject.AddComponent<Watcher>();
        Step();

        var moved = UnityEngine.Object.Instantiate(template.gameObject);
        moved.transform.SetParent(holder, false);
        var born = UnityEngine.Object.Instantiate(template.gameObject, holder);
        holder.sizeDelta = new Vector2(-100, 0);
        Step();
        foreach (var (clone, how) in new[] { (moved, "SetParent after Instantiate"), (born, "Instantiate with a parent") })
        {
            Latest(clone.GetComponent<Watcher>()!, new Vector2(700, 600), $"a clone's script after {how}");
            Latest(clone.transform.GetChild(0).GetComponent<Watcher>()!, new Vector2(700, 600), $"a clone's child script after {how}");
        }
        holder.sizeDelta = Vector2.zero;
        Step();
    }

    private static void Step() => DisunityHost.Step(0.05f);

    private static void Latest(Watcher watcher, Vector2 expected, string what)
    {
        Expect.That(watcher.Sizes.Count > 0, $"{what}: no message arrived");
        var actual = watcher.Sizes[^1];
        Expect.That((actual - expected).magnitude < 1e-3f, $"{what}: expected {expected}, got {actual}");
    }

    private static RectTransform NewRect(RectTransform? parent)
    {
        var rect = DisunityHost.CreateGameObject().AddComponent<RectTransform>();
        if (parent != null) rect.SetParent(parent, false);
        return rect;
    }

    private static RectTransform Stretched(RectTransform parent)
    {
        var rect = NewRect(parent);
        (rect.anchorMin, rect.anchorMax, rect.sizeDelta) = (Vector2.zero, Vector2.one, Vector2.zero);
        return rect;
    }
}
