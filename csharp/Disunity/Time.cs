namespace Disunity;

/// <summary>Frame timing as advanced by the host's fixed-step loop.</summary>
public static class Time
{
    /// <summary>Seconds for the most recent stepped frame.</summary>
    public static float DeltaTime { get; internal set; }

    /// <summary>Total stepped frames since world load.</summary>
    public static uint Frame { get; internal set; }
}
