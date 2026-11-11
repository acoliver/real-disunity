namespace UnityEngine.Playables;

/// <summary>The engine has no playable graphs (the engine's choice, until something evaluates them). Each member that
/// would build or run one throws <see cref="NotSupportedException"/> naming itself.</summary>
internal static class PlayableUnsupported
{
    internal static NotSupportedException Member(string member) => new($"{member} is not supported: the engine has no playable graphs");
}

/// <summary>A node of a playable graph.</summary>
public interface IPlayable
{
    PlayableHandle GetHandle();
}

/// <summary>An output of a playable graph.</summary>
public interface IPlayableOutput
{
    PlayableOutputHandle GetHandle();
}

/// <summary>An asset that creates a playable, such as a timeline or one of its clips.</summary>
public interface IPlayableAsset
{
    double duration { get; }
    IEnumerable<PlayableBinding> outputs { get; }
    Playable CreatePlayable(PlayableGraph graph, GameObject owner);
}

/// <summary>The hooks a graph calls on the behaviour of a script playable.</summary>
public interface IPlayableBehaviour
{
    void OnGraphStart(Playable playable);
    void OnGraphStop(Playable playable);
    void OnPlayableCreate(Playable playable);
    void OnPlayableDestroy(Playable playable);
    void OnBehaviourPlay(Playable playable, FrameData info);
    void OnBehaviourPause(Playable playable, FrameData info);
    void PrepareFrame(Playable playable, FrameData info);
    void ProcessFrame(Playable playable, FrameData info, object? playerData);
}

/// <summary>A message a playable sends to the receivers of its output.</summary>
public interface INotification
{
    PropertyName id { get; }
}

/// <summary>An object that receives the notifications of a playable output.</summary>
public interface INotificationReceiver
{
    void OnNotify(Playable origin, INotification notification, object context);
}
