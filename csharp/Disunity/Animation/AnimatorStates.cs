namespace UnityEngine;

/// <summary>
/// One layer's current or next state. The hashes are <see cref="Animator.StringToHash"/> values of the
/// state's short name, its full path ("Base Layer.Idle") and the path of its parent state machine.
/// The engine plays no animation, so values only come from code that builds them.
/// </summary>
public struct AnimatorStateInfo
{
    private int m_Name;
    private int m_Path;
    private int m_FullPath;
    private float m_NormalizedTime;
    private float m_Length;
    private float m_Speed;
    private float m_SpeedMultiplier;
    private int m_Tag;
    private int m_Loop;

    internal AnimatorStateInfo(int name, int path, int fullPath, float normalizedTime, float length,
        float speed, float speedMultiplier, int tag, bool loop)
    {
        m_Name = name;
        m_Path = path;
        m_FullPath = fullPath;
        m_NormalizedTime = normalizedTime;
        m_Length = length;
        m_Speed = speed;
        m_SpeedMultiplier = speedMultiplier;
        m_Tag = tag;
        m_Loop = loop ? 1 : 0;
    }

    /// <summary>True when the name hashes to the state's short name, full path or parent path.</summary>
    public readonly bool IsName(string name)
    {
        var hash = Animator.StringToHash(name);
        return hash == m_FullPath || hash == m_Name || hash == m_Path;
    }

    public readonly int fullPathHash => m_FullPath;
    public readonly int nameHash => m_Path;
    public readonly int shortNameHash => m_Name;
    public readonly float normalizedTime => m_NormalizedTime;
    public readonly float length => m_Length;
    public readonly float speed => m_Speed;
    public readonly float speedMultiplier => m_SpeedMultiplier;
    public readonly int tagHash => m_Tag;
    public readonly bool IsTag(string tag) => Animator.StringToHash(tag) == m_Tag;
    public readonly bool loop => m_Loop != 0;
}

/// <summary>A clip and its weight in a layer's current blend. The clip is identified by instance ID.</summary>
public struct AnimatorClipInfo
{
    private int m_ClipInstanceID;
    private float m_Weight;

    internal AnimatorClipInfo(AnimationClip clip, float weight)
    {
        m_ClipInstanceID = clip.GetInstanceID();
        m_Weight = weight;
        AnimationClip.Remember(clip);
    }

    public readonly AnimationClip? clip => AnimationClip.FromInstanceID(m_ClipInstanceID);
    public readonly float weight => m_Weight;
}

/// <summary>A transition in progress between two states of a layer.</summary>
public struct AnimatorTransitionInfo
{
    private int m_FullPath;
    private int m_UserName;
    private int m_Name;
    private bool m_HasFixedDuration;
    private float m_Duration;
    private float m_NormalizedTime;
    private bool m_AnyState;
    private int m_TransitionType;

    internal AnimatorTransitionInfo(int fullPath, int userName, int name, bool fixedDuration, float duration,
        float normalizedTime, bool anyState, int transitionType)
    {
        m_FullPath = fullPath;
        m_UserName = userName;
        m_Name = name;
        m_HasFixedDuration = fixedDuration;
        m_Duration = duration;
        m_NormalizedTime = normalizedTime;
        m_AnyState = anyState;
        m_TransitionType = transitionType;
    }

    public readonly bool IsName(string name)
    {
        var hash = Animator.StringToHash(name);
        return hash == m_Name || hash == m_FullPath;
    }

    public readonly bool IsUserName(string name) => Animator.StringToHash(name) == m_UserName;
    public readonly int fullPathHash => m_FullPath;
    public readonly int nameHash => m_Name;
    public readonly int userNameHash => m_UserName;
    public readonly DurationUnit durationUnit => m_HasFixedDuration ? DurationUnit.Fixed : DurationUnit.Normalized;
    public readonly float duration => m_Duration;
    public readonly float normalizedTime => m_NormalizedTime;
    public readonly bool anyState => m_AnyState;
    internal readonly int TransitionType => m_TransitionType;
}

/// <summary>How strongly a position and rotation are matched by <see cref="Animator.MatchTarget(Vector3, Quaternion, AvatarTarget, MatchTargetWeightMask, float)"/>.</summary>
public struct MatchTargetWeightMask
{
    private Vector3 m_PositionXYZWeight;
    private float m_RotationWeight;

    public MatchTargetWeightMask(Vector3 positionXYZWeight, float rotationWeight)
    {
        m_PositionXYZWeight = positionXYZWeight;
        m_RotationWeight = rotationWeight;
    }

    public Vector3 positionXYZWeight { readonly get => m_PositionXYZWeight; set => m_PositionXYZWeight = value; }
    public float rotationWeight { readonly get => m_RotationWeight; set => m_RotationWeight = value; }
}
