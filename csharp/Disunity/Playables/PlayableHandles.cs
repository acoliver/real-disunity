namespace UnityEngine.Playables;

/// <summary>The handle of a playable. The engine creates no playables, so every handle is the null handle and every
/// null handle equals every other.</summary>
public struct PlayableHandle : IEquatable<PlayableHandle>
{
    public static PlayableHandle Null => default;

    public readonly bool IsNull() => true;
    public readonly bool IsValid() => false;
    public readonly Type GetPlayableType() => throw PlayableUnsupported.Member("PlayableHandle.GetPlayableType");

    public readonly bool Equals(PlayableHandle other) => true;
    public override readonly bool Equals(object? obj) => obj is PlayableHandle;
    public override readonly int GetHashCode() => 0;
    public static bool operator ==(PlayableHandle x, PlayableHandle y) => true;
    public static bool operator !=(PlayableHandle x, PlayableHandle y) => false;
}

/// <summary>A node of a playable graph. The engine creates none, so a playable is the null playable.</summary>
public struct Playable : IPlayable, IEquatable<Playable>
{
    public static Playable Null => default;

    public static Playable Create(PlayableGraph graph, int inputCount = 0) => throw PlayableUnsupported.Member("Playable.Create");

    public readonly PlayableHandle GetHandle() => default;
    public readonly bool IsValid() => false;
    public readonly Type GetPlayableType() => throw PlayableUnsupported.Member("Playable.GetPlayableType");
    public readonly bool IsPlayableOfType<T>() where T : struct => throw PlayableUnsupported.Member("Playable.IsPlayableOfType");

    public readonly bool Equals(Playable other) => true;
    public override readonly bool Equals(object? obj) => obj is Playable;
    public override readonly int GetHashCode() => 0;
    public static bool operator ==(Playable x, Playable y) => true;
    public static bool operator !=(Playable x, Playable y) => false;
    public static implicit operator PlayableHandle(Playable playable) => default;
    public static explicit operator Playable(PlayableHandle handle) => default;
}

/// <summary>The handle of a playable output; always the null handle, as for <see cref="PlayableHandle"/>.</summary>
public struct PlayableOutputHandle : IEquatable<PlayableOutputHandle>
{
    public static PlayableOutputHandle Null => default;

    public readonly bool IsNull() => true;
    public readonly bool IsValid() => false;

    public readonly bool Equals(PlayableOutputHandle other) => true;
    public override readonly bool Equals(object? obj) => obj is PlayableOutputHandle;
    public override readonly int GetHashCode() => 0;
    public static bool operator ==(PlayableOutputHandle x, PlayableOutputHandle y) => true;
    public static bool operator !=(PlayableOutputHandle x, PlayableOutputHandle y) => false;
}

/// <summary>An output of a playable graph. The engine creates none, so an output is the null output.</summary>
public struct PlayableOutput : IPlayableOutput
{
    public static PlayableOutput Null => default;

    public readonly PlayableOutputHandle GetHandle() => default;
    public readonly bool IsOutputNull() => true;
    public readonly bool IsOutputValid() => false;

    public static implicit operator PlayableOutputHandle(PlayableOutput output) => default;
    public static explicit operator PlayableOutput(PlayableOutputHandle handle) => default;
}

/// <summary>A playable whose behaviour is a game class. The engine creates none, so it is the null playable.</summary>
public struct ScriptPlayable<T> : IPlayable, IEquatable<ScriptPlayable<T>> where T : class, IPlayableBehaviour, new()
{
    public static ScriptPlayable<T> Null => default;

    public static ScriptPlayable<T> Create(PlayableGraph graph, int inputCount = 0) => throw PlayableUnsupported.Member("ScriptPlayable.Create");
    public static ScriptPlayable<T> Create(PlayableGraph graph, T template, int inputCount = 0) => throw PlayableUnsupported.Member("ScriptPlayable.Create");

    public readonly PlayableHandle GetHandle() => default;
    public readonly T GetBehaviour() => throw PlayableUnsupported.Member("ScriptPlayable.GetBehaviour");
    public readonly bool IsNull() => true;
    public readonly bool IsValid() => false;

    public readonly bool Equals(ScriptPlayable<T> other) => true;
    public override readonly bool Equals(object? obj) => obj is ScriptPlayable<T>;
    public override readonly int GetHashCode() => 0;
    public static implicit operator Playable(ScriptPlayable<T> playable) => default;
    public static implicit operator PlayableHandle(ScriptPlayable<T> playable) => default;
    public static explicit operator ScriptPlayable<T>(Playable playable) => default;
}

/// <summary>An output that carries a script playable's data to game code. The engine creates none.</summary>
public struct ScriptPlayableOutput : IPlayableOutput
{
    public static ScriptPlayableOutput Null => default;

    public static ScriptPlayableOutput Create(PlayableGraph graph, string name) => throw PlayableUnsupported.Member("ScriptPlayableOutput.Create");

    public readonly PlayableOutputHandle GetHandle() => default;
    public readonly bool IsOutputNull() => true;
    public readonly bool IsOutputValid() => false;

    public static implicit operator PlayableOutput(ScriptPlayableOutput output) => default;
    public static explicit operator ScriptPlayableOutput(PlayableOutput output) => default;
}
