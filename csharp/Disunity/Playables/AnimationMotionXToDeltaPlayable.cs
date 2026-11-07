using UnityEngine.Playables;

namespace UnityEngine.Animations;

/// <summary>A playable that turns the motion of an animation clip into a delta. The engine creates none, so it is the null playable.</summary>
public struct AnimationMotionXToDeltaPlayable : IPlayable, IEquatable<AnimationMotionXToDeltaPlayable>
{
    public static AnimationMotionXToDeltaPlayable Null => default;

    public static AnimationMotionXToDeltaPlayable Create(PlayableGraph graph) => throw PlayableUnsupported.Member("AnimationMotionXToDeltaPlayable.Create");

    public readonly PlayableHandle GetHandle() => default;

    public readonly bool Equals(AnimationMotionXToDeltaPlayable other) => true;
    public override readonly bool Equals(object? obj) => obj is AnimationMotionXToDeltaPlayable;
    public override readonly int GetHashCode() => 0;
    public static implicit operator Playable(AnimationMotionXToDeltaPlayable playable) => default;
    public static explicit operator AnimationMotionXToDeltaPlayable(Playable playable) => default;
}
