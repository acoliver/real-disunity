using System.Text.Json;
using Disunity.Play;

/// <summary>The screen size a built player sees: the window the presentation draws into.</summary>
internal static class PlayerScreenTests
{
    public static void Run()
    {
        WindowSizeIsTheScreenTheGameReads();
        NoWindowIsTheDefaultStandaloneSize();
        Console.WriteLine("PASS: the player's screen is the request's window, else 1920 by 1080");
    }

    private static void WindowSizeIsTheScreenTheGameReads()
    {
        var request = Request("""{"screen":[960,540]}""");
        Expect.Equal((960, 540), PlayerScreen.Size(request), "a 960 by 540 window");
    }

    private static void NoWindowIsTheDefaultStandaloneSize()
    {
        Expect.Equal((1920, 1080), PlayerScreen.Size(Request("{}")), "a headless run");
    }

    private static JsonElement Request(string json) => JsonDocument.Parse(json).RootElement;
}
