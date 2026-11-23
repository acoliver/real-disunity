using Disunity.Hosting;
using UnityEngine;

/// <summary>Who hears OnRectTransformDimensionsChange: not a script that is destroyed in the frame or disabled, and a
/// handler that throws does not keep the later listeners from hearing it.</summary>
internal static class RectDimensionDeliveryTests
{
    private sealed class Watcher : MonoBehaviour
    {
        public readonly List<Vector2> Sizes = new();
        private void OnRectTransformDimensionsChange() => Sizes.Add(((RectTransform)transform).rect.size);
    }

    private sealed class Faulty : MonoBehaviour
    {
        private void OnRectTransformDimensionsChange() => throw new InvalidOperationException("the handler failed");
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
            ScriptsDestroyedInTheFrameAreNotTold(holder);
            DisabledScriptsAreNotTold(holder);
            AHandlerThatThrowsDoesNotSilenceTheOthers(holder);
            Console.WriteLine("PASS: OnRectTransformDimensionsChange skips destroyed and disabled scripts and survives a throwing handler");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void ScriptsDestroyedInTheFrameAreNotTold(RectTransform holder)
    {
        var singleWatcher = Stretched(holder).gameObject.AddComponent<Watcher>();
        var survivor = Stretched(holder).gameObject.AddComponent<Watcher>();
        Step();
        var told = singleWatcher.Sizes.Count;

        UnityEngine.Object.Destroy(singleWatcher);
        holder.sizeDelta = new Vector2(-40, 0);
        Canvas.ForceUpdateCanvases();
        Latest(survivor, new Vector2(760, 600), "a script that stays hears the change in the same frame");
        Expect.Equal(told, singleWatcher.Sizes.Count, "a script destroyed in the frame is told nothing");
        Step();
        holder.sizeDelta = Vector2.zero;
        Step();
    }

    private static void DisabledScriptsAreNotTold(RectTransform holder)
    {
        var watcher = Stretched(holder).gameObject.AddComponent<Watcher>();
        Step();
        watcher.enabled = false;
        var told = watcher.Sizes.Count;
        holder.sizeDelta = new Vector2(-30, 0);
        Step();
        Expect.Equal(told, watcher.Sizes.Count, "a disabled script is told nothing");
        watcher.enabled = true;
        Step();
        Latest(watcher, new Vector2(770, 600), "the change made while disabled is reported once the script is enabled");
        holder.sizeDelta = Vector2.zero;
        Step();
    }

    private static void AHandlerThatThrowsDoesNotSilenceTheOthers(RectTransform holder)
    {
        Stretched(holder).gameObject.AddComponent<Faulty>();
        var later = Stretched(holder).gameObject.AddComponent<Watcher>();
        Step();
        var reported = DisunityHost.ScriptExceptionCount;
        holder.sizeDelta = new Vector2(-20, 0);
        Step();
        Expect.Equal(reported + 1, DisunityHost.ScriptExceptionCount, "a game exception in the handler is reported once");
        Latest(later, new Vector2(780, 600), "a later script is told after a handler throws");
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
