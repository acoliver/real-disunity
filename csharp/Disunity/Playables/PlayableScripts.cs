namespace UnityEngine.Playables;

/// <summary>
/// An asset that creates a playable. A game or package class derives from it and implements
/// <see cref="CreatePlayable"/>; it is a script asset, so it is created, enabled and destroyed as any
/// <see cref="ScriptableObject"/> is.
/// </summary>
public abstract class PlayableAsset : ScriptableObject, IPlayableAsset
{
    /// <summary>How long the asset plays. By default it plays until it is stopped, so the duration is infinite.</summary>
    public virtual double duration => PlayableBinding.DefaultDuration;

    /// <summary>The streams the asset's playable outputs to. By default none.</summary>
    public virtual IEnumerable<PlayableBinding> outputs => PlayableBinding.None;

    public abstract Playable CreatePlayable(PlayableGraph graph, GameObject owner);
}

/// <summary>
/// The behaviour of a script playable. Every hook does nothing until a derived class overrides it.
/// </summary>
public class PlayableBehaviour : IPlayableBehaviour, ICloneable
{
    public PlayableBehaviour() { }

    public virtual void OnGraphStart(Playable playable) { }
    public virtual void OnGraphStop(Playable playable) { }
    public virtual void OnPlayableCreate(Playable playable) { }
    public virtual void OnPlayableDestroy(Playable playable) { }
    public virtual void OnBehaviourPlay(Playable playable, FrameData info) { }
    public virtual void OnBehaviourPause(Playable playable, FrameData info) { }
    public virtual void PrepareFrame(Playable playable, FrameData info) { }
    public virtual void ProcessFrame(Playable playable, FrameData info, object? playerData) { }

    /// <summary>A separate behaviour of the same class with the same field values.</summary>
    public virtual object Clone() => MemberwiseClone();
}

/// <summary>Creates the bindings of a script playable's output.</summary>
public static class ScriptPlayableBinding
{
    public static PlayableBinding Create(string name, UnityEngine.Object? key, Type type) => throw PlayableUnsupported.Member("ScriptPlayableBinding.Create");
}
