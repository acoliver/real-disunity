using System.Collections.Concurrent;

namespace UnityEngine.CrashReportHandler;

/// <summary>
/// Crash report settings. The engine sends no crash reports, so the user metadata is kept in memory for
/// the process and goes nowhere else.
/// </summary>
public static class CrashReportHandler
{
    private static readonly ConcurrentDictionary<string, string> Metadata = new(StringComparer.Ordinal);

    /// <summary>Stores a key and its value. A null value removes the key; a null key throws.</summary>
    public static void SetUserMetadata(string key, string? value)
    {
        ArgumentNullException.ThrowIfNull(key);
        if (value is null) Metadata.TryRemove(key, out _);
        else Metadata[key] = value;
    }

    /// <summary>The value stored for a key, or null when the key is not set.</summary>
    public static string? GetUserMetadata(string key)
    {
        ArgumentNullException.ThrowIfNull(key);
        return Metadata.TryGetValue(key, out var value) ? value : null;
    }
}
