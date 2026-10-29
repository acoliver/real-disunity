using Disunity.Hosting;

namespace UnityEngine;

/// <summary>A fraction of Hertz, as Unity's display modes report their refresh rate.</summary>
public struct RefreshRate : IEquatable<RefreshRate>, IComparable<RefreshRate>
{
    public uint numerator;
    public uint denominator;

    /// <summary>Hertz; 0 when the denominator is 0.</summary>
    public readonly double value => denominator == 0 ? 0.0 : (double)numerator / denominator;

    public readonly bool Equals(RefreshRate other) => (ulong)numerator * other.denominator == (ulong)other.numerator * denominator;
    public override readonly bool Equals(object? obj) => obj is RefreshRate other && Equals(other);
    public override readonly int GetHashCode() => value.GetHashCode();
    public readonly int CompareTo(RefreshRate other) => value.CompareTo(other.value);
    public override readonly string ToString() => value.ToString("0.##", System.Globalization.CultureInfo.InvariantCulture) + "Hz";
}

/// <summary>A screen size and refresh rate.</summary>
public struct Resolution
{
    private int m_Width;
    private int m_Height;
    private RefreshRate m_RefreshRate;

    public int width { readonly get => m_Width; set => m_Width = value; }
    public int height { readonly get => m_Height; set => m_Height = value; }
    public RefreshRate refreshRateRatio { readonly get => m_RefreshRate; set => m_RefreshRate = value; }

    /// <summary>Whole Hertz: the refresh rate ratio rounded to the nearest integer. Setting it sets a ratio of value over 1.</summary>
    public int refreshRate
    {
        readonly get => (int)Math.Round(m_RefreshRate.value);
        set => m_RefreshRate = new RefreshRate { numerator = (uint)Math.Max(value, 0), denominator = 1 };
    }

    public override readonly string ToString() => $"{m_Width} x {m_Height} @ {refreshRate}Hz";
}

/// <summary>A display and the size of its work area.</summary>
public struct DisplayInfo : IEquatable<DisplayInfo>
{
    public int width;
    public int height;
    public RefreshRate refreshRate;
    public RectInt workArea;
    public string name;
    public float physicalDpi;

    /// <summary>The resolutions the display offers: its current one.</summary>
    public readonly Resolution[] resolutions => [new Resolution { width = width, height = height, refreshRateRatio = refreshRate }];

    public readonly bool Equals(DisplayInfo other) =>
        width == other.width && height == other.height && refreshRate.Equals(other.refreshRate)
        && workArea.Equals(other.workArea) && name == other.name && physicalDpi == other.physicalDpi;
    public override readonly bool Equals(object? obj) => obj is DisplayInfo other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(width, height, name);

    /// <summary>Replaces the list's contents with the displays: the one the engine has.</summary>
    public static void GetLayout(List<DisplayInfo> displayLayout)
    {
        ArgumentNullException.ThrowIfNull(displayLayout);
        displayLayout.Clear();
        displayLayout.Add(Screen.mainWindowDisplayInfo);
    }
}

/// <summary>
/// The presentation surface. Width, height, density and refresh rate come from the host
/// (<see cref="DisunityHost.SetScreen"/>), and the engine has one display that the window fills. The
/// window cannot be resized or moved yet, so SetResolution records the request and logs once that
/// the size is kept; full-screen changes are recorded.
/// </summary>
public sealed class Screen
{
    private static FullScreenMode _fullScreenMode = FullScreenMode.Windowed;
    private static int _msaaSamples = 1;
    private static bool _resizeLogged;

    public static int width => (int)DisunityHost.ViewportWidth;
    public static int height => (int)DisunityHost.ViewportHeight;
    public static float dpi => DisunityHost.ScreenDpi;

    /// <summary>Always landscape: the engine runs on desktop, where Unity ignores changes.</summary>
    public static ScreenOrientation orientation { get => ScreenOrientation.LandscapeLeft; set { } }

    public static Resolution currentResolution => new() { width = width, height = height, refreshRateRatio = DisunityHost.ScreenRefreshRate };
    public static Resolution[] resolutions => [currentResolution];

    public static FullScreenMode fullScreenMode { get => _fullScreenMode; set => _fullScreenMode = value; }
    public static bool fullScreen { get => _fullScreenMode != FullScreenMode.Windowed; set => _fullScreenMode = value ? FullScreenMode.FullScreenWindow : FullScreenMode.Windowed; }

    public static int msaaSamples => _msaaSamples;

    /// <summary>Records the sample count. Unity accepts 0, 1, 2, 4 and 8; others throw here.</summary>
    public static void SetMSAASamples(int numSamples)
    {
        if (numSamples is not (0 or 1 or 2 or 4 or 8)) throw new ArgumentOutOfRangeException(nameof(numSamples), "MSAA sample counts are 0, 1, 2, 4 and 8");
        _msaaSamples = numSamples;
    }

    public static Rect safeArea => new(0, 0, width, height);
    public static Rect[] cutouts => [];
    public static Vector2Int mainWindowPosition => Vector2Int.zero;

    public static DisplayInfo mainWindowDisplayInfo => new()
    {
        width = width,
        height = height,
        refreshRate = DisunityHost.ScreenRefreshRate,
        workArea = new RectInt(0, 0, width, height),
        name = "Display 1",
        physicalDpi = dpi,
    };

    public static void GetDisplayLayout(List<DisplayInfo> displayLayout) => DisplayInfo.GetLayout(displayLayout);

    /// <summary>The window cannot move yet.</summary>
    public static AsyncOperation MoveMainWindowTo(ref DisplayInfo display, Vector2Int position) =>
        throw new NotSupportedException("Screen.MoveMainWindowTo is not supported yet");

    public static void SetResolution(int width, int height, FullScreenMode fullscreenMode, RefreshRate preferredRefreshRate)
    {
        if (width <= 0 || height <= 0) throw new ArgumentOutOfRangeException(nameof(width), "a resolution has pixels");
        _fullScreenMode = fullscreenMode;
        if (_resizeLogged) return;
        _resizeLogged = true;
        Debug.LogWarning($"Screen.SetResolution({width}, {height}) recorded the full-screen mode only; the window keeps its {Screen.width} x {Screen.height} size.");
    }

    public static void SetResolution(int width, int height, FullScreenMode fullscreenMode, int preferredRefreshRate) =>
        SetResolution(width, height, fullscreenMode, new RefreshRate { numerator = (uint)Math.Max(preferredRefreshRate, 0), denominator = 1 });

    public static void SetResolution(int width, int height, FullScreenMode fullscreenMode) =>
        SetResolution(width, height, fullscreenMode, 0);

    public static void SetResolution(int width, int height, bool fullscreen, int preferredRefreshRate) =>
        SetResolution(width, height, fullscreen ? FullScreenMode.FullScreenWindow : FullScreenMode.Windowed, preferredRefreshRate);

    public static void SetResolution(int width, int height, bool fullscreen) =>
        SetResolution(width, height, fullscreen, 0);
}

/// <summary>The engine's one display, which the presentation window fills.</summary>
public sealed class Display
{
    public delegate void DisplaysUpdatedDelegate();

    /// <summary>The engine's one display. Unity declares this as a field, so scripts bind to a field.</summary>
    public static Display[] displays = [new Display()];
    private static readonly Display Main = displays[0];
    public static Display main => Main;
    public static event DisplaysUpdatedDelegate? onDisplaysUpdated { add { } remove { } }

    private Display() { }

    public int renderingWidth => Screen.width;
    public int renderingHeight => Screen.height;
    public int systemWidth => Screen.width;
    public int systemHeight => Screen.height;
    public RenderBuffer colorBuffer => default;
    public RenderBuffer depthBuffer => default;
    public bool active => true;
    public bool requiresBlitToBackbuffer => false;
    public bool requiresSrgbBlitToBackbuffer => false;

    /// <summary>The main display is always active.</summary>
    public void Activate() { }
    public void Activate(int width, int height, RefreshRate refreshRate) { }
    public void Activate(int width, int height, int refreshRate) { }
    public void SetParams(int width, int height, int x, int y) => throw new NotSupportedException("Display.SetParams is not supported yet");
    public void SetRenderingResolution(int w, int h) => throw new NotSupportedException("Display.SetRenderingResolution is not supported yet");
    public static bool MultiDisplayLicense() => false;

    /// <summary>Mouse coordinates relative to the display under the cursor: with one display, the input with display index 0 in z.</summary>
    public static Vector3 RelativeMouseAt(Vector3 inputMouseCoordinates) => new(inputMouseCoordinates.x, inputMouseCoordinates.y, 0f);
}
