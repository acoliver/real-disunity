using Disunity.Hosting;
using UnityEngine;

/// <summary>A script on a scene object that unloading destroys reads its panel's rectangle from the <c>OnDestroy</c> that
/// the teardown advance runs. The panel awaits destruction too and stays usable until it retires, so the read succeeds and
/// unloading completes. The teardown advance runs no <c>Update</c>, so a surviving script's <c>Update</c> does not run.</summary>
internal static class DestroyedRectTests
{
    private sealed class Survivor : MonoBehaviour
    {
        public RectTransform? Panel;
        public bool Armed;
        public int Reads;
        public Vector2 LastSize;
        public string? Error;

        private void OnDestroy()
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
            survivor = loaded.Objects[0].AddComponent<Survivor>();
            survivor.Panel = panel;
            survivor.Armed = true;
        }
        finally { DisunityHost.UnloadWorld(); }
        Expect.That(survivor.Error is null, $"reading the rectangle of a destroyed panel failed: {survivor.Error}");
        Expect.That(survivor.Reads > 0, "the reader's OnDestroy read the panel during the teardown advance");
        Expect.That(survivor.LastSize == new Vector2(120, 45), $"the destroyed panel kept its size, got {survivor.LastSize}");
        Console.WriteLine("PASS: a destroyed panel's rectangle stays readable until unloading retires it");
    }
}
