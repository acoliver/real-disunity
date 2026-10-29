using System.Text.Json;
using Disunity.Play;

/// <summary>The command line a built player reads through <c>Environment.GetCommandLineArgs()</c>.</summary>
internal static class PlayerCommandLineTests
{
    /// <summary>An absolute directory on any platform, with forward slashes as the player's paths use.</summary>
    private static readonly string Root = Path.GetFullPath(Path.Combine(Path.GetTempPath(), "games")).Replace('\\', '/');

    public static void Run()
    {
        ExecutablePathFollowsPlatformAndDataDirectory();
        ArgumentsFollowTheExecutableInOrder();
        InstalledLineIsWhatTheGameReads();
        Console.WriteLine("PASS: the player's command line is its executable path followed by the request's arguments");
    }

    private static void ExecutablePathFollowsPlatformAndDataDirectory()
    {
        Expect.Equal($"{Root}/Old World/OldWorld.exe", PlayerCommandLine.ExecutablePath($"{Root}/Old World/OldWorld_Data", "WindowsPlayer"), "Windows build");
        Expect.Equal($"{Root}/OldWorld/OldWorld", PlayerCommandLine.ExecutablePath($"{Root}/OldWorld/OldWorld_Data/", "LinuxPlayer"), "Linux build, trailing slash");
        Expect.Equal($"{Root}/Tool", PlayerCommandLine.ExecutablePath($"{Root}/Tool", "LinuxPlayer"), "a directory without the suffix keeps its name");
        Expect.Equal($"{Root}/_Data", PlayerCommandLine.ExecutablePath($"{Root}/_Data", "LinuxPlayer"), "the suffix alone is not stripped to an empty name");
    }

    private static void ArgumentsFollowTheExecutableInOrder()
    {
        var request = Request($$"""{"player":"{{Root}}/g/Game_Data","identity":{"platform":"WindowsPlayer"},"arguments":["-userdata","/tmp/a b","-headless","-userdata","x"]}""");
        Expect.Sequence(new[] { $"{Root}/g/Game.exe", "-userdata", "/tmp/a b", "-headless", "-userdata", "x" }, PlayerCommandLine.Build(request), "command line");
        var none = Request($$"""{"player":"{{Root}}/g/Game_Data","identity":{"platform":"LinuxPlayer"},"arguments":[]}""");
        Expect.Sequence(new[] { $"{Root}/g/Game" }, PlayerCommandLine.Build(none), "no arguments");
    }

    private static void InstalledLineIsWhatTheGameReads()
    {
        var original = Environment.GetCommandLineArgs();
        try
        {
            PlayerCommandLine.Install(new[] { "/g/Game", "game.zip", "-headless" });
            Expect.Sequence(new[] { "/g/Game", "game.zip", "-headless" }, Environment.GetCommandLineArgs(), "Environment.GetCommandLineArgs");
        }
        finally { PlayerCommandLine.Install(original); }
    }

    private static JsonElement Request(string json) => JsonDocument.Parse(json).RootElement;
}
