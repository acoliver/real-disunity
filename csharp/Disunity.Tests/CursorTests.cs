using Disunity.Hosting;
using UnityEngine;

/// <summary>Cursor state, the default cursor and the reset when a world loads.</summary>
internal static class CursorTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            VisibilityAndLock();
            DefaultCursor();
        }
        finally { DisunityHost.UnloadWorld(); }
        ResetsWhenAWorldLoads();
        Console.WriteLine("PASS: Cursor visibility, lock state, default cursor and reset on world load");
    }

    private static void Defaults()
    {
        Expect.That(Cursor.visible && DisunityHost.CursorVisible, "the cursor starts visible");
        Expect.Equal(CursorLockMode.None, Cursor.lockState, "lockState");
        Expect.Equal(CursorLockMode.None, DisunityHost.CursorLockState, "host lockState");
    }

    private static void VisibilityAndLock()
    {
        Cursor.visible = false;
        Cursor.lockState = CursorLockMode.Locked;
        Expect.That(!Cursor.visible && !DisunityHost.CursorVisible, "visible set");
        Expect.Equal(CursorLockMode.Locked, Cursor.lockState, "lockState set");
        Expect.Equal(CursorLockMode.Locked, DisunityHost.CursorLockState, "the host reads the lock state");
        Cursor.lockState = CursorLockMode.Confined;
        Expect.Equal(CursorLockMode.Confined, Cursor.lockState, "Confined");
        Expect.Throws<ArgumentOutOfRangeException>(() => Cursor.lockState = (CursorLockMode)7, "undefined lock mode");
        Expect.Equal(CursorLockMode.Confined, Cursor.lockState, "a rejected mode changes nothing");
    }

    private static void DefaultCursor()
    {
        Cursor.SetCursor(null!, new Vector2(3, 4), CursorMode.ForceSoftware);
        Expect.That(DisunityHost.CursorHotspot == new Vector2(3, 4), "the hotspot is stored");
        Expect.Equal(CursorMode.ForceSoftware, DisunityHost.CursorMode, "the mode is stored");
        var texture = new Texture2D(2, 2);
        var error = Expect.Catch<NotSupportedException>(() => Cursor.SetCursor(texture, Vector2.zero, CursorMode.Auto), "custom cursor texture");
        Expect.That(error.Message.Contains("custom cursor textures are not supported yet"), error.Message);
        Expect.That(DisunityHost.CursorHotspot == new Vector2(3, 4), "a refused texture changes nothing");
    }

    private static void ResetsWhenAWorldLoads()
    {
        Cursor.visible = false;
        Cursor.lockState = CursorLockMode.Locked;
        DisunityHost.LoadWorld();
        try
        {
            Expect.That(Cursor.visible, "visible reset");
            Expect.Equal(CursorLockMode.None, Cursor.lockState, "lockState reset");
            Expect.That(DisunityHost.CursorHotspot == Vector2.zero, "hotspot reset");
            Expect.Equal(CursorMode.Auto, DisunityHost.CursorMode, "mode reset");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
