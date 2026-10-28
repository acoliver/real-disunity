using System.Runtime.InteropServices;

namespace UnityEngine;

/// <summary>One key of an <see cref="AnimationCurve"/>: a value at a time with the slopes of the curve
/// arriving at and leaving it. Weights only take effect when <see cref="weightedMode"/> names that side.</summary>
[StructLayout(LayoutKind.Sequential)]
public struct Keyframe
{
    private float m_Time;
    private float m_Value;
    private float m_InTangent;
    private float m_OutTangent;
    private int m_WeightedMode;
    private float m_InWeight;
    private float m_OutWeight;
    private int m_TangentMode;

    /// <summary>A key with flat tangents and no weights.</summary>
    public Keyframe(float time, float value)
    {
        m_Time = time;
        m_Value = value;
        m_InTangent = 0f;
        m_OutTangent = 0f;
        m_WeightedMode = (int)WeightedMode.None;
        m_InWeight = 0f;
        m_OutWeight = 0f;
        m_TangentMode = 0;
    }

    public Keyframe(float time, float value, float inTangent, float outTangent)
    {
        m_Time = time;
        m_Value = value;
        m_InTangent = inTangent;
        m_OutTangent = outTangent;
        m_WeightedMode = (int)WeightedMode.None;
        m_InWeight = 0f;
        m_OutWeight = 0f;
        m_TangentMode = 0;
    }

    /// <summary>A key with explicit weights; this form weights both sides, as Unity's does.</summary>
    public Keyframe(float time, float value, float inTangent, float outTangent, float inWeight, float outWeight)
    {
        m_Time = time;
        m_Value = value;
        m_InTangent = inTangent;
        m_OutTangent = outTangent;
        m_WeightedMode = (int)WeightedMode.Both;
        m_InWeight = inWeight;
        m_OutWeight = outWeight;
        m_TangentMode = 0;
    }

    public float time { readonly get => m_Time; set => m_Time = value; }
    public float value { readonly get => m_Value; set => m_Value = value; }
    public float inTangent { readonly get => m_InTangent; set => m_InTangent = value; }
    public float outTangent { readonly get => m_OutTangent; set => m_OutTangent = value; }
    public float inWeight { readonly get => m_InWeight; set => m_InWeight = value; }
    public float outWeight { readonly get => m_OutWeight; set => m_OutWeight = value; }

    public WeightedMode weightedMode
    {
        readonly get => (WeightedMode)m_WeightedMode;
        set => m_WeightedMode = (int)value;
    }

    /// <summary>The editor's tangent mode; a player always reads what was stored, 0 unless set.</summary>
    [Obsolete("Use AnimationUtility.SetKeyLeftTangentMode, AnimationUtility.SetKeyRightTangentMode, AnimationUtility.GetKeyLeftTangentMode or AnimationUtility.GetKeyRightTangentMode instead.")]
    public int tangentMode { readonly get => m_TangentMode; set => m_TangentMode = value; }

    internal readonly bool SameAs(Keyframe other) =>
        m_Time.Equals(other.m_Time) && m_Value.Equals(other.m_Value)
        && m_InTangent.Equals(other.m_InTangent) && m_OutTangent.Equals(other.m_OutTangent)
        && m_WeightedMode == other.m_WeightedMode && m_InWeight.Equals(other.m_InWeight)
        && m_OutWeight.Equals(other.m_OutWeight) && m_TangentMode == other.m_TangentMode;

    internal readonly int Hash() => HashCode.Combine(m_Time, m_Value, m_InTangent, m_OutTangent,
        HashCode.Combine(m_WeightedMode, m_InWeight, m_OutWeight, m_TangentMode));
}
