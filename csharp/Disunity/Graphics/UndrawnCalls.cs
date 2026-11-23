namespace UnityEngine;

/// <summary>
/// The draw entry points the game reaches complete without drawing until the native draw surface exists (#171).
/// That is the engine's choice: the call validates its arguments, leaves render texture contents unchanged, runs no
/// render pipeline callback, and logs a warning naming the member the first time it is called in a world.
/// The same once-per-world policy logs each distinct reason a host's GPU flush fails.
/// </summary>
internal static class UndrawnCalls
{
    private static readonly HashSet<string> Noted = new();

    /// <summary>Logs the member's warning on its first call since the world was loaded.</summary>
    public static void Note(string member) => WarnOnce(member, $"{member} draws nothing yet: the engine has no draw surface");

    /// <summary>
    /// Logs the reason a GPU flush failed the first time that reason is seen since the world was loaded. The call that asked
    /// for the flush goes on with the contents the CPU holds, so a host whose device is lost does not fail every read.
    /// </summary>
    public static void NoteFlushFailure(string reason) => WarnOnce("GPU flush: " + reason, $"{reason}; the call used the contents the CPU holds");

    /// <summary>Logs the first time a texture is left out because the host cannot store its bytes; <paramref name="reason"/>
    /// says why.</summary>
    public static void NoteTextureUnstored(string name, string reason) =>
        WarnOnce("texture unstored: " + name, $"texture '{name}' {reason}, so it is sent as no texture");

    /// <summary>Logs the first time a batch renderer group's culling callback or a batch draw fails for a reason, since the world
    /// was loaded. The cull or the record goes on without that group's or that draw's batches.</summary>
    public static void NoteBatchFailure(string reason) => WarnOnce("batch: " + reason, $"BatchRendererGroup: {reason}; its batches are left out");

    private static void WarnOnce(string key, string message)
    {
        lock (Noted)
        {
            if (!Noted.Add(key)) return;
        }
        Debug.LogWarning(message);
    }

    /// <summary>Forgets the members already noted, so the next world notes each again.</summary>
    public static void Reset()
    {
        lock (Noted) Noted.Clear();
    }
}
