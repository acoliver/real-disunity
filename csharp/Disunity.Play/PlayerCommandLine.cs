using System.Reflection;
using System.Text.Json;

namespace Disunity.Play;

/// <summary>
/// The command line a built player receives. <c>Environment.GetCommandLineArgs()</c> reports the player's
/// own command line instead of the one of the hosted runtime: argument 0 is the player's executable
/// path, then the arguments the request lists, in order.
/// </summary>
internal static class PlayerCommandLine
{
    /// <summary>The runtime's own copy of the command line; no public API sets it for a hosted runtime.</summary>
    private const string FieldName = "s_commandLineArgs";

    /// <summary>
    /// The player's executable path. The engine chooses it: the data directory's parent joined with the
    /// directory's name without its <c>_Data</c> suffix, plus <c>.exe</c> for a WindowsPlayer build.
    /// </summary>
    internal static string ExecutablePath(string data, string platform)
    {
        var full = Path.GetFullPath(data).Replace('\\', '/').TrimEnd('/');
        var slash = full.LastIndexOf('/');
        var name = full[(slash + 1)..];
        if (name.EndsWith("_Data", StringComparison.Ordinal) && name.Length > "_Data".Length) name = name[..^"_Data".Length];
        var parent = slash <= 0 ? full[..(slash + 1)] : full[..slash];
        var path = parent.EndsWith('/') ? parent + name : parent + "/" + name;
        return platform == "WindowsPlayer" ? path + ".exe" : path;
    }

    /// <summary>The player's command line from its request: the executable path, then the listed arguments.</summary>
    internal static string[] Build(JsonElement request)
    {
        var data = request.GetProperty("player").GetString()!;
        var platform = request.GetProperty("identity").GetProperty("platform").GetString()!;
        var line = new List<string> { ExecutablePath(data, platform) };
        if (request.TryGetProperty("arguments", out var arguments))
            line.AddRange(arguments.EnumerateArray().Select(argument => argument.GetString()!));
        return line.ToArray();
    }

    /// <summary>Makes <c>Environment.GetCommandLineArgs()</c> return <paramref name="line"/> for the rest of the process.</summary>
    internal static void Install(string[] line)
    {
        var field = typeof(Environment).GetField(FieldName, BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException(
                $"cannot set the player's command line: this runtime's System.Environment has no static field {FieldName}");
        field.SetValue(null, line);
    }
}
