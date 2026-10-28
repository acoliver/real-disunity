namespace UnityEngine;

/// <summary>The base of the assets an Animator plays. A bare Motion has no duration, speed or looping.</summary>
public class Motion : Object
{
    public virtual float averageDuration => 0f;
    public virtual float averageAngularSpeed => 0f;
    public virtual Vector3 averageSpeed => Vector3.zero;
    public virtual float apparentSpeed => 0f;
    public virtual bool isLooping => false;
    public bool legacy => IsLegacy;
    public virtual bool isHumanMotion => false;
    public virtual bool isAnimatorMotion => !IsLegacy;

    /// <summary>False: retargeting needs a humanoid avatar, which the engine does not load.</summary>
    public bool ValidateIfRetargetable(bool val) => false;

    internal virtual bool IsLegacy => false;
}
