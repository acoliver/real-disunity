namespace UnityEngine.Identifiers;

/// <summary>The device identifiers the services packages read.</summary>
public static class Identifiers
{
    private static readonly string Installation = Guid.NewGuid().ToString();

    /// <summary>An identifier of this installation: a random identifier made once per run, since the engine keeps no record of
    /// an installation between runs (the engine's choice). It does not change within a run.</summary>
    public static string installationId => Installation;
}
