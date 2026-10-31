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
        Cursor.SetCursor(texture, new Vector2(1, 2), CursorMode.Auto);
        Expect.That(ReferenceEquals(DisunityHost.CursorTexture, texture), "the cursor texture is stored for a presentation host");
        Expect.That(DisunityHost.CursorHotspot == new Vector2(1, 2), "its hotspot is stored");
        Expect.Throws<ArgumentOutOfRangeException>(() => Cursor.SetCursor(texture, Vector2.zero, (CursorMode)9), "undefined cursor mode");
        Expect.That(DisunityHost.CursorHotspot == new Vector2(1, 2), "a rejected mode changes nothing");
        Cursor.SetCursor(null!, Vector2.zero, CursorMode.Auto);
        Expect.That(DisunityHost.CursorTexture is null, "a null texture restores the default cursor");
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
            Expect.That(DisunityHost.CursorTexture is null, "texture reset");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
