using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// What a game learns about where it runs: <see cref="Application"/>'s environment and
/// lifetime, logging through <see cref="Debug"/> and its log events, and
/// <see cref="PlayerPrefs"/> kept between runs. Configuring a player starts a new lifetime, so
/// quitting can be checked more than once.
/// </summary>
internal static class EnvironmentTests
{
    public static void Run()
    {
        ReportsTheDefaultEnvironment();
        var directory = Directory.CreateTempSubdirectory("disunity-environment-");
        try
        {
            ReportsAPlayersEnvironment(directory.FullName);
            KeepsPreferencesBetweenRuns(directory.FullName);
        }
        finally
        {
            DisunityHost.ConfigurePlayer(PlayerEnvironment.Default);
            directory.Delete(recursive: true);
        }
        DeliversLogMessages();
        LogsWithoutAWorld();
        AsksBeforeQuittingAndSavesPreferences();
        Console.WriteLine("PASS: Application, Debug and PlayerPrefs report and keep the game's environment");
    }

    private static void ReportsTheDefaultEnvironment()
    {
        Require(Application.unityVersion == PlayerEnvironment.EngineUnityVersion, "the engine reports its Unity release");
        Require(!Application.dataPath.Contains('\\') && Application.streamingAssetsPath == Application.dataPath + "/StreamingAssets",
            "paths use forward slashes");
        var expected = OperatingSystem.IsWindows() ? RuntimePlatform.WindowsPlayer
            : OperatingSystem.IsMacOS() ? RuntimePlatform.OSXPlayer : RuntimePlatform.LinuxPlayer;
        Require(Application.platform == expected && Application.isPlaying && !Application.isEditor, "the engine runs as a player");
        Require((int)RuntimePlatform.WindowsPlayer == 2 && (int)LogType.Exception == 4 && (int)SystemLanguage.English == 10,
            "engine enums have Unity's values");
    }

    private static void ReportsAPlayersEnvironment(string directory)
    {
        DisunityHost.ConfigurePlayer(new PlayerEnvironment("/games/Demo_Data", directory, "Studio", "Demo", "1.2.3",
            "6000.3.15f1", RuntimePlatform.WindowsPlayer));
        Require((Application.companyName, Application.productName, Application.version) == ("Studio", "Demo", "1.2.3"),
            "names and version come from the player");
        Require(Application.dataPath == "/games/Demo_Data" && Application.streamingAssetsPath == "/games/Demo_Data/StreamingAssets"
            && Application.persistentDataPath == directory, "paths come from the player");
        Require(Application.platform == RuntimePlatform.WindowsPlayer, "the platform is the one the player was built for");
    }

    private static void KeepsPreferencesBetweenRuns(string directory)
    {
        PlayerPrefs.SetInt("volume", 7);
        PlayerPrefs.SetFloat("gamma", 1.25f);
        PlayerPrefs.SetString("name", "Ada");
        Require(PlayerPrefs.GetInt("volume") == 7 && PlayerPrefs.GetFloat("gamma") == 1.25f && PlayerPrefs.GetString("name") == "Ada",
            "values read back by their type");
        Require(PlayerPrefs.GetFloat("volume", -1) == -1 && PlayerPrefs.GetInt("gamma", -1) == -1 && PlayerPrefs.GetString("missing", "x") == "x",
            "a key of another type or no key gives the default");
        PlayerPrefs.Save();
        PlayerPrefs.SetInt("unsaved", 1);
        PlayerPrefs.DeleteKey("name");
        DisunityHost.ConfigurePlayer(DisunityHost.GameEnvironment);
        Require(PlayerPrefs.GetInt("volume") == 7 && PlayerPrefs.GetFloat("gamma") == 1.25f && PlayerPrefs.HasKey("name")
            && !PlayerPrefs.HasKey("unsaved"), "saved values come back in the next run and unsaved ones do not");
        Require(File.Exists(Path.Combine(directory, "PlayerPrefs.xml")), "preferences are saved in the persistent data path");
        PlayerPrefs.DeleteAll();
        Require(!PlayerPrefs.HasKey("volume"), "DeleteAll removes every key");
        File.WriteAllText(Path.Combine(directory, "PlayerPrefs.xml"), "<unity_prefs><pref name=\"x\" type=\"int\">many</pref>");
        DisunityHost.ConfigurePlayer(DisunityHost.GameEnvironment);
        Require(!PlayerPrefs.HasKey("x"), "a file that cannot be read is ignored");
    }

    private static void DeliversLogMessages()
    {
        var received = new List<(string Condition, bool Stack, LogType Type)>();
        var threaded = new List<string>();
        void OnMessage(string condition, string stack, LogType type) => received.Add((condition, stack.Length > 0, type));
        void OnThreaded(string condition, string stack, LogType type) => threaded.Add(condition);
        Application.logMessageReceived += OnMessage;
        Application.logMessageReceivedThreaded += OnThreaded;
        DisunityHost.LoadWorld();
        try
        {
            var culture = System.Globalization.CultureInfo.CurrentCulture;
            System.Globalization.CultureInfo.CurrentCulture = new System.Globalization.CultureInfo("de-DE");
            try { Debug.Log(1.5f); }
            finally { System.Globalization.CultureInfo.CurrentCulture = culture; }
            Require(received[^1].Condition == "1.5", $"values format with the invariant culture: {received[^1].Condition}");
            Debug.LogFormat("literal {{braces}}");
            Require(received[^1].Condition == "literal {braces}", "a format string is always formatted");
            received.Clear();
            Debug.Log("plain");
            Debug.LogWarningFormat("{0} {1}", "formatted", 2);
            Debug.unityLogger.Log("Tag", "tagged");
            Debug.LogException(new InvalidOperationException("broken"));
            Debug.unityLogger.filterLogType = LogType.Warning;
            Debug.Log("filtered");
            Debug.LogError("error");
            Debug.unityLogger.filterLogType = LogType.Log;
            Require(received.SequenceEqual([("plain", false, LogType.Log), ("formatted 2", true, LogType.Warning),
                ("Tag: tagged", false, LogType.Log), ("InvalidOperationException: broken", false, LogType.Exception),
                ("error", true, LogType.Error)]), string.Join(" | ", received));
            received.Clear();
            Task.Run(() => Debug.Log("from a worker")).Wait();
            Require(threaded[^1] == "from a worker" && received.Count == 0, "a worker's message reaches the threaded event at once");
            DisunityHost.Step(0.02f);
            Require(received is [("from a worker", false, LogType.Log)], "and the main thread event in the next frame");
        }
        finally
        {
            Application.logMessageReceived -= OnMessage;
            Application.logMessageReceivedThreaded -= OnThreaded;
            DisunityHost.UnloadWorld();
        }
    }

    private static void AsksBeforeQuittingAndSavesPreferences()
    {
        var directory = Directory.CreateTempSubdirectory("disunity-quit-");
        DisunityHost.ConfigurePlayer(PlayerEnvironment.Default with { PersistentDataPath = directory.FullName });
        PlayerPrefs.SetInt("set before quitting", 1);
        var quitting = 0;
        var allow = false;
        Func<bool> ask = () => allow;
        Application.wantsToQuit += ask;
        Application.quitting += () => quitting++;
        Application.Quit(3);
        Require(quitting == 0 && !Application.exitCancellationToken.IsCancellationRequested, "a handler can refuse to quit");
        allow = true;
        Application.Quit(3);
        Require(quitting == 1 && Application.exitCancellationToken.IsCancellationRequested, "quitting runs and cancels the exit token");
        Application.Quit();
        Require(quitting == 1, "the game quits once");
        Application.wantsToQuit -= ask;
        Require(File.ReadAllText(Path.Combine(directory.FullName, "PlayerPrefs.xml")).Contains("set before quitting"),
            "quitting saves the preferences");
        DisunityHost.ConfigurePlayer(PlayerEnvironment.Default);
        Require(!Application.exitCancellationToken.IsCancellationRequested, "configuring a player starts a new lifetime");
        Func<bool> broken = () => throw new InvalidOperationException("handler failed");
        Application.wantsToQuit += broken;
        try
        {
            Application.Quit();
            Require(quitting == 2 && Application.exitCancellationToken.IsCancellationRequested,
                "a handler that throws is logged and the game still quits");
        }
        finally { Application.wantsToQuit -= broken; }
        DisunityHost.ConfigurePlayer(PlayerEnvironment.Default);
        directory.Delete(recursive: true);
    }

    /// <summary>Without a loaded world a message from another thread is delivered at once.</summary>
    private static void LogsWithoutAWorld()
    {
        var received = new List<string>();
        void OnMessage(string condition, string stack, LogType type) => received.Add(condition);
        Application.logMessageReceived += OnMessage;
        try { Task.Run(() => Debug.Log("no world")).Wait(); }
        finally { Application.logMessageReceived -= OnMessage; }
        Require(received is ["no world"], "a message logged without a world is delivered");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"environment: {message}");
    }
}
