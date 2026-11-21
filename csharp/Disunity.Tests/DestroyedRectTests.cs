using Disunity.Hosting;
using UnityEngine;

/// <summary>A surviving script reads the rectangle of a scene object that unloading has just destroyed. The object stays
/// usable until the end of the frame, so the read succeeds and unloading completes.</summary>
internal static class DestroyedRectTests
{
    private sealed class Survivor : MonoBehaviour
    {
        public RectTransform? Panel;
        public bool Armed;
        public int Reads;
        public Vector2 LastSize;
        public string? Error;

        private void Update()
        {
            if (!Armed || Panel is null) return;
            try
            {
                LastSize = Panel.rect.size;
                Reads++;
            }
            catch (Exception failure) { Error = failure.Message; }
        }
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        Survivor survivor;
        try
        {
            using var loaded = PlayerCameraPlanTests.Load("");
            var panel = loaded.Objects[0].AddComponent<RectTransform>();
            panel.sizeDelta = new Vector2(120, 45);
            survivor = DisunityHost.AddComponent(new Survivor());
            UnityEngine.Object.DontDestroyOnLoad(survivor.gameObject);
            survivor.Panel = panel;
            survivor.Armed = true;
        }
        finally { DisunityHost.UnloadWorld(); }
        Expect.That(survivor.Error is null, $"reading the rectangle of a destroyed panel failed: {survivor.Error}");
        Expect.That(survivor.Reads > 0, "the survivor's Update ran during the cleanup advance");
        Expect.That(survivor.LastSize == new Vector2(120, 45), $"the destroyed panel kept its size, got {survivor.LastSize}");
        Console.WriteLine("PASS: a destroyed panel's rectangle stays readable until unloading retires it");
    }
}
