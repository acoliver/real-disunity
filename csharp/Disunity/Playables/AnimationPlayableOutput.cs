using UnityEngine.Playables;

namespace UnityEngine.Animations;

/// <summary>An output that drives an <see cref="Animator"/> from a playable graph. The engine creates none.</summary>
public struct AnimationPlayableOutput : IPlayableOutput
{
    public static AnimationPlayableOutput Null => default;

    public static AnimationPlayableOutput Create(PlayableGraph graph, string name, Animator target) =>
        throw PlayableUnsupported.Member("AnimationPlayableOutput.Create");

    public readonly PlayableOutputHandle GetHandle() => default;
    public readonly bool IsOutputNull() => true;
    public readonly bool IsOutputValid() => false;
    public readonly Animator GetTarget() => throw PlayableUnsupported.Member("AnimationPlayableOutput.GetTarget");
    public readonly void SetTarget(Animator value) => throw PlayableUnsupported.Member("AnimationPlayableOutput.SetTarget");

    public static implicit operator PlayableOutput(AnimationPlayableOutput output) => default;
    public static explicit operator AnimationPlayableOutput(PlayableOutput output) => default;
}
