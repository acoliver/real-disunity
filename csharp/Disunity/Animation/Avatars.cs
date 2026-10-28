namespace UnityEngine;

/// <summary>A rig description. The engine loads no avatars, so none exists to hand out and none is valid.</summary>
public class Avatar : Object
{
    private readonly HumanDescription? _description;

    internal Avatar(HumanDescription? description) => _description = description;

    public bool isValid => _description is not null;
    public bool isHuman => _description is not null;
    public HumanDescription humanDescription => _description ?? default;
}

/// <summary>Which body parts and transforms an animation layer affects. Transform paths are stored as given.</summary>
public sealed class AvatarMask : Object
{
    private readonly bool[] _bodyParts = Enumerable.Repeat(true, (int)AvatarMaskBodyPart.LastBodyPart).ToArray();
    private readonly List<(string Path, bool Active)> _transforms = new();

    public AvatarMask() { }

    public int humanoidBodyPartCount => _bodyParts.Length;

    public int transformCount
    {
        get => _transforms.Count;
        set
        {
            ArgumentOutOfRangeException.ThrowIfNegative(value);
            if (value < _transforms.Count) _transforms.RemoveRange(value, _transforms.Count - value);
            while (_transforms.Count < value) _transforms.Add(("", true));
        }
    }

    public bool GetHumanoidBodyPartActive(AvatarMaskBodyPart index) => _bodyParts[BodyPart(index)];
    public void SetHumanoidBodyPartActive(AvatarMaskBodyPart index, bool value) => _bodyParts[BodyPart(index)] = value;
    public string GetTransformPath(int index) => _transforms[Slot(index)].Path;
    public void SetTransformPath(int index, string path) => _transforms[Slot(index)] = (path ?? throw new ArgumentNullException(nameof(path)), _transforms[index].Active);
    public bool GetTransformActive(int index) => _transforms[Slot(index)].Active;
    public void SetTransformActive(int index, bool value) => _transforms[Slot(index)] = (_transforms[index].Path, value);

    public void AddTransformPath(Transform transform) => AddTransformPath(transform, true);
    public void AddTransformPath(Transform transform, bool recursive) => throw Paths(nameof(AddTransformPath));
    public void RemoveTransformPath(Transform transform) => RemoveTransformPath(transform, true);
    public void RemoveTransformPath(Transform transform, bool recursive) => throw Paths(nameof(RemoveTransformPath));

    private static NotSupportedException Paths(string member) => new(
        $"AvatarMask.{member} is not supported yet: transform paths are relative to an Animator's root, which the engine does not have; use SetTransformPath");

    private static int BodyPart(AvatarMaskBodyPart index) =>
        index is >= 0 and < AvatarMaskBodyPart.LastBodyPart ? (int)index : throw new ArgumentOutOfRangeException(nameof(index));

    private int Slot(int index) =>
        (uint)index < (uint)_transforms.Count ? index : throw new ArgumentOutOfRangeException(nameof(index));
}

/// <summary>The humanoid retargeting settings of an avatar.</summary>
public struct HumanDescription
{
    public HumanBone[] human;
    public SkeletonBone[] skeleton;
    private float m_ArmTwist;
    private float m_ForeArmTwist;
    private float m_UpperLegTwist;
    private float m_LegTwist;
    private float m_ArmStretch;
    private float m_LegStretch;
    private float m_FeetSpacing;
    private float m_GlobalScale;
    private string m_RootMotionBoneName;
    private bool m_HasTranslationDoF;
    private bool m_HasExtraRoot;
    private bool m_SkeletonHasParents;

    public float upperArmTwist { readonly get => m_ArmTwist; set => m_ArmTwist = value; }
    public float lowerArmTwist { readonly get => m_ForeArmTwist; set => m_ForeArmTwist = value; }
    public float upperLegTwist { readonly get => m_UpperLegTwist; set => m_UpperLegTwist = value; }
    public float lowerLegTwist { readonly get => m_LegTwist; set => m_LegTwist = value; }
    public float armStretch { readonly get => m_ArmStretch; set => m_ArmStretch = value; }
    public float legStretch { readonly get => m_LegStretch; set => m_LegStretch = value; }
    public float feetSpacing { readonly get => m_FeetSpacing; set => m_FeetSpacing = value; }
    public bool hasTranslationDoF { readonly get => m_HasTranslationDoF; set => m_HasTranslationDoF = value; }
    internal float GlobalScale { readonly get => m_GlobalScale; set => m_GlobalScale = value; }
    internal string RootMotionBoneName { readonly get => m_RootMotionBoneName; set => m_RootMotionBoneName = value; }
    internal bool HasExtraRoot { readonly get => m_HasExtraRoot; set => m_HasExtraRoot = value; }
    internal bool SkeletonHasParents { readonly get => m_SkeletonHasParents; set => m_SkeletonHasParents = value; }
}

/// <summary>Maps one skeleton bone to a humanoid bone.</summary>
public struct HumanBone
{
    private string m_BoneName;
    private string m_HumanName;
    public HumanLimit limit;

    public string boneName { readonly get => m_BoneName; set => m_BoneName = value; }
    public string humanName { readonly get => m_HumanName; set => m_HumanName = value; }
}

/// <summary>A bone's rest pose in an avatar's skeleton.</summary>
public struct SkeletonBone
{
    public string name;
    public Vector3 position;
    public Quaternion rotation;
    public Vector3 scale;
    private string? m_ParentName;
    private int m_TransformModified;

    public int transformModified { readonly get => m_TransformModified; set => m_TransformModified = value; }
    internal string? ParentName { readonly get => m_ParentName; set => m_ParentName = value; }
}

/// <summary>The rotation range of a humanoid bone.</summary>
public struct HumanLimit
{
    private Vector3 m_Min;
    private Vector3 m_Max;
    private Vector3 m_Center;
    private float m_AxisLength;
    private int m_UseDefaultValues;

    public bool useDefaultValues { readonly get => m_UseDefaultValues != 0; set => m_UseDefaultValues = value ? 1 : 0; }
    public Vector3 min { readonly get => m_Min; set => m_Min = value; }
    public Vector3 max { readonly get => m_Max; set => m_Max = value; }
    public Vector3 center { readonly get => m_Center; set => m_Center = value; }
    public float axisLength { readonly get => m_AxisLength; set => m_AxisLength = value; }
}
