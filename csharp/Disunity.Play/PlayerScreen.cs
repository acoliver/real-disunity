using System.Text.Json;

namespace Disunity.Play;

/// <summary>The screen size a built player reads from <c>Screen</c>.</summary>
internal static class PlayerScreen
{
    /// <summary>
    /// The request's <c>screen</c> (width and height of the presentation window), or 1920 by 1080 when the request
    /// names none: a headless run has no window, and 1920 by 1080 is the engine's choice for it.
    /// </summary>
    internal static (int Width, int Height) Size(JsonElement request)
    {
        if (!request.TryGetProperty("screen", out var screen)) return (1920, 1080);
        return (screen[0].GetInt32(), screen[1].GetInt32());
    }
}
