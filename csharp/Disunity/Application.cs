using Disunity.Hosting;
using Events = UnityEngine.Events;

namespace UnityEngine;

/// <summary>
/// The running game's environment and lifetime. Paths, names and versions come from
/// <see cref="DisunityHost.GameEnvironment"/>. The engine runs as a desktop player with focus, no
/// network check and no store installer. <see cref="Quit()"/> asks
/// <see cref="wantsToQuit"/>, raises <see cref="quitting"/>, saves <see cref="PlayerPrefs"/> and
/// cancels <see cref="exitCancellationToken"/>; the host decides when the run stops.
/// Configuring a player starts its lifetime afresh.
/// </summary>
public static class Application
{
    public delegate void LogCallback(string condition, string stackTrace, LogType type);
    public delegate void LowMemoryCallback();

    private static CancellationTokenSource Exit = new();
    private static int _targetFrameRate = -1;

    public static string dataPath => DisunityHost.GameEnvironment.DataPath;
    public static string streamingAssetsPath => DisunityHost.GameEnvironment.StreamingAssetsPath;
    public static string persistentDataPath => DisunityHost.GameEnvironment.PersistentDataPath;
    public static string temporaryCachePath => Path.Combine(Path.GetTempPath(), companyName, productName).Replace('\\', '/');
    public static string consoleLogPath => "";
    public static string absoluteURL => "";
    public static string companyName => DisunityHost.GameEnvironment.CompanyName;
    public static string productName => DisunityHost.GameEnvironment.ProductName;
    public static string identifier => $"com.{companyName}.{productName}";
    public static string version => DisunityHost.GameEnvironment.Version;
    public static string unityVersion => DisunityHost.GameEnvironment.UnityVersion;
    public static string buildGUID => "";
    public static string cloudProjectId => "";
    public static string installerName => "";
    public static ApplicationInstallMode installMode => ApplicationInstallMode.Unknown;
    public static ApplicationSandboxType sandboxType => ApplicationSandboxType.NotSandboxed;
    public static RuntimePlatform platform => DisunityHost.GameEnvironment.Platform;
    public static bool isMobilePlatform => false;
    public static bool isConsolePlatform => false;
    public static bool isEditor => false;
    public static bool isPlaying => true;
    public static bool isFocused => true;
    public static bool isBatchMode => false;
    public static bool isShowingSplashScreen => false;
    public static bool genuine => true;
    public static bool genuineCheckAvailable => false;
    public static bool runInBackground { get; set; }
    public static SystemLanguage systemLanguage => SystemLanguage.English;
    public static NetworkReachability internetReachability => NetworkReachability.NotReachable;
    public static ThreadPriority backgroundLoadingPriority { get; set; } = ThreadPriority.Normal;

    /// <summary>Cancelled when the game quits.</summary>
    public static CancellationToken exitCancellationToken => Exit.Token;

    /// <summary>The frame rate the game asks for; -1 is the platform default. Frame pacing is
    /// the host's, so this is only recorded.</summary>
    public static int targetFrameRate
    {
        get => _targetFrameRate;
        set => _targetFrameRate = value;
    }

    public static event LogCallback? logMessageReceived;
    public static event LogCallback? logMessageReceivedThreaded;
    public static event Func<bool>? wantsToQuit;
    public static event Action? quitting;

    // The engine has no source for these events, so it never raises them.
    public static event LowMemoryCallback? lowMemory { add { } remove { } }
    public static event Action<bool>? focusChanged { add { } remove { } }
    public static event Action<string>? deepLinkActivated { add { } remove { } }
    public static event Action? unloading { add { } remove { } }
    public static event Events.UnityAction? onBeforeRender { add { } remove { } }

    /// <summary>Whether the game has quit, and with what exit code.</summary>
    internal static int? ExitCode { get; private set; }

    public static void Quit() => Quit(0);

    /// <summary>A handler that throws is logged, as in Unity; for <see cref="wantsToQuit"/> it
    /// counts as allowing the quit.</summary>
    public static void Quit(int exitCode)
    {
        if (ExitCode is not null) return;
        foreach (var ask in wantsToQuit?.GetInvocationList().Cast<Func<bool>>() ?? [])
            if (!Allows(ask)) return;
        ExitCode = exitCode;
        try
        {
            foreach (var handler in quitting?.GetInvocationList().Cast<Action>() ?? [])
                Guarded(handler);
            // Unity saves preferences when the game quits, after the quitting handlers.
            PlayerPrefs.Save();
        }
        finally { Exit.Cancel(); }
    }

    private static bool Allows(Func<bool> ask)
    {
        try { return ask(); }
        catch (Exception error)
        {
            Debug.LogException(error);
            return true;
        }
    }

    private static void Guarded(Action handler)
    {
        try { handler(); }
        catch (Exception error) { Debug.LogException(error); }
    }

    /// <summary>Starts a new lifetime: not quit, with a fresh exit token.</summary>
    internal static void ResetLifetime()
    {
        ExitCode = null;
        Exit = new CancellationTokenSource();
    }

    public static void CancelQuit() { }

    /// <summary>The engine opens no browser; the request is logged.</summary>
    public static void OpenURL(string url) => Debug.Log($"[Application] OpenURL {url}");

    public static bool CanStreamedLevelBeLoaded(int levelIndex) => levelIndex >= 0;
    public static bool HasProLicense() => true;
    public static bool HasUserAuthorization(UserAuthorization mode) => false;
    public static string[] GetBuildTags() => [];
    public static bool IsPlaying(Object obj) => true;

    public static StackTraceLogType GetStackTraceLogType(LogType logType) =>
        logType == LogType.Log ? StackTraceLogType.None : StackTraceLogType.ScriptOnly;

    public static void SetStackTraceLogType(LogType logType, StackTraceLogType stackTraceType) { }

    /// <summary>Delivers a logged message: <see cref="logMessageReceivedThreaded"/> on the
    /// logging thread, and <see cref="logMessageReceived"/> on the main thread, at once when
    /// logged there and otherwise in the next frame.</summary>
    internal static void Deliver(string condition, string stackTrace, LogType type)
    {
        logMessageReceivedThreaded?.Invoke(condition, stackTrace, type);
        if (logMessageReceived is not { } handlers) return;
        // Without a loaded world there is no frame to deliver in, so delivery is immediate.
        if (DisunityHost.OnMainThread || !DisunityHost.PostToMainThread(() => logMessageReceived?.Invoke(condition, stackTrace, type)))
            handlers(condition, stackTrace, type);
    }
}
