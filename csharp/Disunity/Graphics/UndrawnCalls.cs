namespace UnityEngine;

/// <summary>
/// The draw entry points the game reaches complete without drawing until the native draw surface exists (#171).
/// That is the engine's choice: the call validates its arguments, leaves render texture contents unchanged, runs no
/// render pipeline callback, and logs a warning naming the member the first time it is called in a world.
/// </summary>
internal static class UndrawnCalls
{
    private static readonly HashSet<string> Noted = new();

    /// <summary>Logs the member's warning on its first call since the world was loaded.</summary>
    public static void Note(string member)
    {
        lock (Noted)
        {
            if (!Noted.Add(member)) return;
        }
        Debug.LogWarning($"{member} draws nothing yet: the engine has no draw surface");
    }

    /// <summary>Forgets the members already noted, so the next world notes each again.</summary>
    public static void Reset()
    {
        lock (Noted) Noted.Clear();
    }
}
