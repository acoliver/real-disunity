using Disunity.Hosting;
using UnityEngine;

/// <summary>A script that defines OnRectTransformDimensionsChange hears about every change to the size of its
/// rectangle, whether the change comes from its own values, an ancestor's or the screen. Layout groups, text and
/// graphics lay out again on this message; without it they keep the size they were first laid out at.</summary>
internal static class RectDimensionMessageTests
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
            var holder = Stretched(canvas);
            var panel = Stretched(holder);
            var watcher = panel.gameObject.AddComponent<Watcher>();
            var other = Stretched(holder).gameObject.AddComponent<Watcher>();
            Step();
            Latest(watcher, new Vector2(800, 600), "the first frame reports the size laid out");

            DisunityHost.SetViewport(1280, 720);
            Step();
            Latest(watcher, new Vector2(1280, 720), "a new screen size reaches a descendant of the canvas");

            holder.sizeDelta = new Vector2(-80, -20);
            Step();
            Latest(watcher, new Vector2(1200, 700), "a parent's size change reaches its stretched child");

            panel.sizeDelta = new Vector2(-200, 0);
            Step();
            Latest(watcher, new Vector2(1000, 700), "a change to the rectangle's own size");

            var count = watcher.Sizes.Count;
            panel.anchoredPosition = new Vector2(30, 40);
            panel.sizeDelta = new Vector2(-200, 0);
            Step();
            Expect.Equal(count, watcher.Sizes.Count, "a move that leaves the size alone sends nothing");

            canvas.gameObject.GetComponent<Canvas>()!.scaleFactor = 2f;
            Step();
            Latest(watcher, new Vector2(360, 340), "a canvas scale change resizes the rectangles below it");

            panel.gameObject.SetActive(false);
            holder.sizeDelta = Vector2.zero;
            count = watcher.Sizes.Count;
            Step();
            Expect.Equal(count, watcher.Sizes.Count, "an inactive object is told nothing");
            panel.gameObject.SetActive(true);
            Step();
            Latest(watcher, new Vector2(440, 360), "the change made while inactive is reported once it is active");
            Latest(other, new Vector2(640, 360), "a sibling script hears the same ancestor change");
            Console.WriteLine("PASS: OnRectTransformDimensionsChange reaches a script when its rectangle's size changes");
        }
        finally { DisunityHost.UnloadWorld(); }
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
