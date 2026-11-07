namespace UnityEngine.Playables;

/// <summary>When a graph advances its time.</summary>
public enum DirectorUpdateMode
{
    DSPClock = 0, GameTime = 1, UnscaledGameTime = 2, Manual = 3,
}

/// <summary>
/// A graph of playables. The engine does not evaluate graphs, so none can be created: a default graph is not valid,
/// and creating, playing or reading one throws <see cref="NotSupportedException"/> naming the member.
/// </summary>
public struct PlayableGraph
{
    public static PlayableGraph Create() => throw PlayableUnsupported.Member("PlayableGraph.Create");
    public static PlayableGraph Create(string name) => throw PlayableUnsupported.Member("PlayableGraph.Create");

    public readonly bool IsValid() => false;
    public readonly bool IsPlaying() => throw PlayableUnsupported.Member("PlayableGraph.IsPlaying");
    public readonly bool IsDone() => throw PlayableUnsupported.Member("PlayableGraph.IsDone");
    public readonly void Play() => throw PlayableUnsupported.Member("PlayableGraph.Play");
    public readonly void Stop() => throw PlayableUnsupported.Member("PlayableGraph.Stop");
    public readonly void Evaluate() => throw PlayableUnsupported.Member("PlayableGraph.Evaluate");
    public readonly void Evaluate(float deltaTime) => throw PlayableUnsupported.Member("PlayableGraph.Evaluate");
    public readonly void Destroy() => throw PlayableUnsupported.Member("PlayableGraph.Destroy");
    public readonly int GetRootPlayableCount() => throw PlayableUnsupported.Member("PlayableGraph.GetRootPlayableCount");
    public readonly Playable GetRootPlayable(int index) => throw PlayableUnsupported.Member("PlayableGraph.GetRootPlayable");
    public readonly int GetOutputCount() => throw PlayableUnsupported.Member("PlayableGraph.GetOutputCount");
    public readonly PlayableOutput GetOutput(int index) => throw PlayableUnsupported.Member("PlayableGraph.GetOutput");
    public readonly DirectorUpdateMode GetTimeUpdateMode() => throw PlayableUnsupported.Member("PlayableGraph.GetTimeUpdateMode");
    public readonly void SetTimeUpdateMode(DirectorUpdateMode value) => throw PlayableUnsupported.Member("PlayableGraph.SetTimeUpdateMode");
    public readonly IExposedPropertyTable GetResolver() => throw PlayableUnsupported.Member("PlayableGraph.GetResolver");
    public readonly void SetResolver(IExposedPropertyTable value) => throw PlayableUnsupported.Member("PlayableGraph.SetResolver");
}
