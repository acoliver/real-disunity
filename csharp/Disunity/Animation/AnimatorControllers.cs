namespace UnityEngine;

/// <summary>An Animator's controller. The engine loads no controller assets, so the base class has no clips.</summary>
public class RuntimeAnimatorController : Object
{
    public virtual AnimationClip[] animationClips => [];
}

/// <summary>An original clip and the clip that replaces it, as the older <c>clips</c> property reports them.</summary>
public class AnimationClipPair
{
    public AnimationClip? originalClip;
    public AnimationClip? overrideClip;
    public AnimationClipPair() { }
}

/// <summary>
/// A controller that replaces some clips of another. The overrides are data: the original clips are those
/// the base controller lists, reading a clip gives its override or else the original, and an override of
/// null removes it. Clips the base controller does not list are ignored with a warning.
/// </summary>
public sealed class AnimatorOverrideController : RuntimeAnimatorController
{
    private readonly Dictionary<AnimationClip, AnimationClip> _overrides = new(ReferenceEqualityComparer.Instance);

    public AnimatorOverrideController() { }
    public AnimatorOverrideController(RuntimeAnimatorController controller) => runtimeAnimatorController = controller;

    public RuntimeAnimatorController? runtimeAnimatorController { get; set; }
    public int overridesCount => Originals().Length;
    public override AnimationClip[] animationClips => Originals().Select(Effective).ToArray()!;

    public AnimationClip? this[string name]
    {
        get => Originals().FirstOrDefault(c => c.name == name) is { } original ? Effective(original) : null;
        set
        {
            if (Originals().FirstOrDefault(c => c.name == name) is { } original) Override(original, value);
            else Debug.LogWarning($"AnimatorOverrideController has no clip named {name} to override.", this);
        }
    }

    public AnimationClip? this[AnimationClip clip]
    {
        get => Array.IndexOf(Originals(), clip) >= 0 ? Effective(clip) : null;
        set
        {
            if (Array.IndexOf(Originals(), clip) >= 0) Override(clip, value);
            else Debug.LogWarning($"AnimatorOverrideController has no clip {clip?.name} to override.", this);
        }
    }

    [Obsolete("Use overridesCount, GetOverrides and ApplyOverrides instead.")]
    public AnimationClipPair[] clips
    {
        get => Originals().Select(o => new AnimationClipPair { originalClip = o, overrideClip = Replacement(o) }).ToArray();
        set
        {
            ArgumentNullException.ThrowIfNull(value);
            ApplyOverrides(value.Select(p => new KeyValuePair<AnimationClip, AnimationClip>(p.originalClip!, p.overrideClip!)).ToList());
        }
    }

    /// <summary>Replaces the list's content with each original clip and its override, or null where there is none.</summary>
    public void GetOverrides(List<KeyValuePair<AnimationClip, AnimationClip>> overrides)
    {
        ArgumentNullException.ThrowIfNull(overrides);
        overrides.Clear();
        foreach (var original in Originals()) overrides.Add(new(original, Replacement(original)!));
    }

    public void ApplyOverrides(IList<KeyValuePair<AnimationClip, AnimationClip>> overrides)
    {
        ArgumentNullException.ThrowIfNull(overrides);
        foreach (var pair in overrides) this[pair.Key] = pair.Value;
    }

    private AnimationClip[] Originals() => runtimeAnimatorController?.animationClips ?? [];
    private AnimationClip? Replacement(AnimationClip original) => _overrides.GetValueOrDefault(original);
    private AnimationClip Effective(AnimationClip original) => Replacement(original) ?? original;

    private void Override(AnimationClip original, AnimationClip? replacement)
    {
        if (replacement is null) _overrides.Remove(original);
        else _overrides[original] = replacement;
    }
}

/// <summary>A parameter of an Animator's controller. The engine loads no controllers, so none has parameters.</summary>
public class AnimatorControllerParameter
{
    private string _name = "";

    public AnimatorControllerParameter() { }

    public string name => _name;
    public int nameHash => Animator.StringToHash(_name);
    public AnimatorControllerParameterType type { get; set; }
    public float defaultFloat { get; set; }
    public int defaultInt { get; set; }
    public bool defaultBool { get; set; }

    internal AnimatorControllerParameter(string name) => _name = name;

    public override bool Equals(object? o) => o is AnimatorControllerParameter other
        && other._name == _name && other.type == type && other.defaultFloat.Equals(defaultFloat)
        && other.defaultInt == defaultInt && other.defaultBool == defaultBool;

    public override int GetHashCode() => _name.GetHashCode();
}
