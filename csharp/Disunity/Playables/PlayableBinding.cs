namespace UnityEngine.Playables;

/// <summary>A stream an asset's playable outputs to, and the object that stands for it.</summary>
public struct PlayableBinding
{
    /// <summary>The duration of an asset that does not state one: infinite.</summary>
    public const double DefaultDuration = double.PositiveInfinity;

    /// <summary>No bindings.</summary>
    public static readonly PlayableBinding[] None = [];

    public enum DataStreamType
    {
        Animation = 0, Audio = 1, Texture = 2, None = 3, ScriptNotification = 4,
    }

    public string? streamName { get; set; }
    public DataStreamType streamType { get; set; }
    public UnityEngine.Object? sourceObject { get; set; }
    public Type? outputTargetType { get; set; }
}

/// <summary>The state a graph passes to a playable's hooks for one frame.</summary>
public struct FrameData
{
    public enum EvaluationType
    {
        Evaluate = 0, Playback = 1,
    }

    public ulong frameId { get; init; }
    public float deltaTime { get; init; }
    public float weight { get; init; }
    public float effectiveWeight { get; init; }
    public double effectiveParentDelay { get; init; }
    public double effectiveParentSpeed { get; init; }
    public double effectiveSpeed { get; init; }
    public EvaluationType evaluationType { get; init; }
    public bool seekOccurred { get; init; }
    public bool timeLooped { get; init; }
    public bool timeHeld { get; init; }
    public PlayState effectivePlayState { get; init; }
}
