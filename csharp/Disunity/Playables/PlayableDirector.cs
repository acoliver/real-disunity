namespace UnityEngine.Playables;

/// <summary>
/// The component that plays a playable asset such as a timeline. The engine does not evaluate playable graphs, so
/// a director keeps its enabled flag and nothing else: every member that would play, seek, bind or read playback
/// state throws <see cref="NotSupportedException"/> naming the member.
/// </summary>
public sealed class PlayableDirector : Behaviour, IExposedPropertyTable
{
    public PlayableDirector() { }

    private protected override bool EnabledState { get; set; } = true;

    private static NotSupportedException Unsupported(string member) => PlayableUnsupported.Member($"PlayableDirector.{member}");

    public PlayState state => throw Unsupported(nameof(state));
    public DirectorWrapMode extrapolationMode { get => throw Unsupported(nameof(extrapolationMode)); set => throw Unsupported(nameof(extrapolationMode)); }
    public PlayableAsset? playableAsset { get => throw Unsupported(nameof(playableAsset)); set => throw Unsupported(nameof(playableAsset)); }
    public PlayableGraph playableGraph => throw Unsupported(nameof(playableGraph));
    public bool playOnAwake { get => throw Unsupported(nameof(playOnAwake)); set => throw Unsupported(nameof(playOnAwake)); }
    public DirectorUpdateMode timeUpdateMode { get => throw Unsupported(nameof(timeUpdateMode)); set => throw Unsupported(nameof(timeUpdateMode)); }
    public double initialTime { get => throw Unsupported(nameof(initialTime)); set => throw Unsupported(nameof(initialTime)); }
    public double time { get => throw Unsupported(nameof(time)); set => throw Unsupported(nameof(time)); }
    public double duration => throw Unsupported(nameof(duration));

    public event Action<PlayableDirector> played { add => throw Unsupported(nameof(played)); remove => throw Unsupported(nameof(played)); }
    public event Action<PlayableDirector> paused { add => throw Unsupported(nameof(paused)); remove => throw Unsupported(nameof(paused)); }
    public event Action<PlayableDirector> stopped { add => throw Unsupported(nameof(stopped)); remove => throw Unsupported(nameof(stopped)); }

    public void Play() => throw Unsupported(nameof(Play));
    public void Play(PlayableAsset asset) => throw Unsupported(nameof(Play));
    public void Play(PlayableAsset asset, DirectorWrapMode mode) => throw Unsupported(nameof(Play));
    public void Stop() => throw Unsupported(nameof(Stop));
    public void Pause() => throw Unsupported(nameof(Pause));
    public void Resume() => throw Unsupported(nameof(Resume));
    public void Evaluate() => throw Unsupported(nameof(Evaluate));
    public void DeferredEvaluate() => throw Unsupported(nameof(DeferredEvaluate));
    public void RebuildGraph() => throw Unsupported(nameof(RebuildGraph));
    public void RebindPlayableGraphOutputs() => throw Unsupported(nameof(RebindPlayableGraphOutputs));
    public void SetGenericBinding(UnityEngine.Object key, UnityEngine.Object? value) => throw Unsupported(nameof(SetGenericBinding));
    public UnityEngine.Object? GetGenericBinding(UnityEngine.Object key) => throw Unsupported(nameof(GetGenericBinding));
    public void ClearGenericBinding(UnityEngine.Object key) => throw Unsupported(nameof(ClearGenericBinding));

    public void SetReferenceValue(PropertyName id, UnityEngine.Object value) => throw Unsupported(nameof(SetReferenceValue));
    public UnityEngine.Object GetReferenceValue(PropertyName id, out bool idValid) => throw Unsupported(nameof(GetReferenceValue));
    public void ClearReferenceValue(PropertyName id) => throw Unsupported(nameof(ClearReferenceValue));
}
