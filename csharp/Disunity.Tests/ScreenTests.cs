using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;

/// <summary>Screen, Display, Resolution, RefreshRate and DisplayInfo.</summary>
internal static class ScreenTests
{
    public static void Run()
    {
        try
        {
            Defaults();
            SetScreenSizesEverything();
            ResolutionAndRefreshRate();
            Console.WriteLine("PASS: Screen sizing from the host, Resolution, RefreshRate and DisplayInfo");
            SetResolutionAndModes();
            DisplayMembers();
            Console.WriteLine("PASS: Screen.SetResolution, full-screen modes, MSAA, Display and unsupported window moves");
        }
        finally
        {
            DisunityHost.SetScreen(800, 600, 96f, 60f);
        }
    }

    private static void Defaults()
    {
        Ensure(Screen.width == 800 && Screen.height == 600, "a host with no window runs at the 800 by 600 viewport");
        Ensure(Screen.dpi == 96f && Screen.orientation == ScreenOrientation.LandscapeLeft, "desktop defaults");
        Ensure(!Screen.fullScreen && Screen.fullScreenMode == FullScreenMode.Windowed && Screen.msaaSamples == 1, "window defaults");
        Ensure(Screen.safeArea == new Rect(0, 0, 800, 600) && Screen.cutouts.Length == 0 && Screen.mainWindowPosition == Vector2Int.zero, "safe area is the whole screen");
    }

    private static void SetScreenSizesEverything()
    {
        DisunityHost.SetScreen(1920, 1080, 110f, 144f);
        Ensure(Screen.width == 1920 && Screen.height == 1080 && Screen.dpi == 110f, "SetScreen sets the size and density");
        var current = Screen.currentResolution;
        Ensure(current.width == 1920 && current.height == 1080 && current.refreshRate == 144, "current resolution");
        Ensure(Screen.resolutions.Length == 1 && Screen.resolutions[0].width == 1920, "resolutions list the current one");
        Ensure(Display.main.systemWidth == 1920 && Display.main.renderingHeight == 1080, "Display reports the screen size");
        Throws<ArgumentOutOfRangeException>(() => DisunityHost.SetScreen(0, 1080, 96f, 60f), "zero width", "width");
        Throws<ArgumentOutOfRangeException>(() => DisunityHost.SetScreen(1920, 1080, 0f, 60f), "zero dpi", "dpi");
        Throws<ArgumentOutOfRangeException>(() => DisunityHost.SetScreen(1920, 1080, 96f, float.NaN), "NaN refresh rate", "refreshRate");
        Ensure(Screen.width == 1920, "a rejected SetScreen changes nothing");
    }

    private static void ResolutionAndRefreshRate()
    {
        var rate = new RefreshRate { numerator = 60000, denominator = 1001 };
        Ensure(Math.Abs(rate.value - 59.94) < 0.001 && rate.ToString() == "59.94Hz", "fractional refresh rate");
        Ensure(new RefreshRate { numerator = 120, denominator = 2 }.Equals(new RefreshRate { numerator = 60, denominator = 1 }), "equal ratios compare equal");
        Ensure(rate.CompareTo(new RefreshRate { numerator = 60, denominator = 1 }) < 0 && new RefreshRate().value == 0, "ordering and zero denominator");
        Ensure(new RefreshRate { numerator = 120, denominator = 2 }.GetHashCode() == new RefreshRate { numerator = 60, denominator = 1 }.GetHashCode(), "equal ratios hash equal");
        var resolution = new Resolution { width = 1280, height = 720, refreshRateRatio = rate };
        Ensure(resolution.refreshRate == 60 && resolution.ToString() == "1280 x 720 @ 60Hz", "Resolution text");
        resolution.refreshRate = 75;
        Ensure(resolution.refreshRateRatio.numerator == 75 && resolution.refreshRateRatio.denominator == 1, "refreshRate setter writes a ratio");
        var info = Screen.mainWindowDisplayInfo;
        Ensure(info.width == Screen.width && info.height == Screen.height && info.physicalDpi == Screen.dpi, "display info matches the screen");
        Ensure(info.workArea == new RectInt(0, 0, Screen.width, Screen.height) && info.name.Length > 0, "work area and name");
        Ensure(info.resolutions.Length == 1 && info.resolutions[0].width == info.width, "display info resolutions");
        var layout = new List<DisplayInfo> { default, default };
        Screen.GetDisplayLayout(layout);
        Ensure(layout.Count == 1 && layout[0].Equals(info), "GetDisplayLayout replaces the list with the one display");
        DisplayInfo.GetLayout(layout);
        Ensure(layout.Count == 1, "DisplayInfo.GetLayout does the same");
    }

    private static void SetResolutionAndModes()
    {
        using (var log = new LogRecorder())
        {
            Screen.SetResolution(1280, 720, FullScreenMode.ExclusiveFullScreen);
            Screen.SetResolution(1024, 768, FullScreenMode.MaximizedWindow, 60);
            Ensure(log.Messages.Count == 1 && log.Messages[0].Contains("keeps its"), $"the window-size warning is logged once: {string.Join("|", log.Messages)}");
        }
        Ensure(Screen.width == 1920 && Screen.height == 1080, "SetResolution does not resize the window");
        Ensure(Screen.fullScreenMode == FullScreenMode.MaximizedWindow && Screen.fullScreen, "the full-screen mode is recorded");
        Screen.SetResolution(1280, 720, false);
        Ensure(!Screen.fullScreen && Screen.fullScreenMode == FullScreenMode.Windowed, "bool overload leaves full screen");
        Screen.SetResolution(1280, 720, true, 60);
        Ensure(Screen.fullScreenMode == FullScreenMode.FullScreenWindow, "bool overload enters full screen");
        Screen.fullScreen = false;
        Ensure(Screen.fullScreenMode == FullScreenMode.Windowed, "fullScreen setter");
        Screen.fullScreenMode = FullScreenMode.ExclusiveFullScreen;
        Ensure(Screen.fullScreen, "fullScreenMode setter");
        Screen.SetResolution(1280, 720, FullScreenMode.Windowed, new RefreshRate { numerator = 60, denominator = 1 });
        Throws<ArgumentOutOfRangeException>(() => Screen.SetResolution(0, 720, false), "zero width", "width");
        Screen.SetMSAASamples(4);
        Ensure(Screen.msaaSamples == 4, "MSAA samples recorded");
        Throws<ArgumentOutOfRangeException>(() => Screen.SetMSAASamples(3), "three samples", "numSamples");
        Ensure(Screen.msaaSamples == 4, "a rejected sample count changes nothing");
        Screen.orientation = ScreenOrientation.Portrait;
        Ensure(Screen.orientation == ScreenOrientation.LandscapeLeft, "desktop ignores orientation changes");
    }

    private static void DisplayMembers()
    {
        Ensure(Display.displays.Length == 1 && Display.main == Display.displays[0], "one display");
        Ensure(typeof(Display).GetField(nameof(Display.displays)) is { IsStatic: true, IsPublic: true }, "displays is a public static field, as in Unity");
        var display = Display.main;
        Ensure(display.active && !display.requiresBlitToBackbuffer && !display.requiresSrgbBlitToBackbuffer, "display flags");
        Ensure(display.colorBuffer.GetNativeRenderBufferPtr() == IntPtr.Zero && display.depthBuffer.GetNativeRenderBufferPtr() == IntPtr.Zero, "default buffers");
        display.Activate();
        display.Activate(640, 480, 60);
        Ensure(!Display.MultiDisplayLicense(), "no multi-display license");
        Ensure(Display.RelativeMouseAt(new Vector3(12, 34, 0)) == new Vector3(12, 34, 0), "mouse on the only display");
        var moved = Screen.mainWindowDisplayInfo;
        NotSupported(() => Screen.MoveMainWindowTo(ref moved, Vector2Int.zero), "MoveMainWindowTo", "Screen.MoveMainWindowTo");
        NotSupported(() => display.SetParams(1, 1, 0, 0), "SetParams", "Display.SetParams");
        NotSupported(() => display.SetRenderingResolution(1, 1), "SetRenderingResolution", "Display.SetRenderingResolution");
        Display.onDisplaysUpdated += () => { };
    }
}
