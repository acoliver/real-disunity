using Disunity.Hosting;

namespace UnityEngine;

public partial class Animator
{
    private bool _enabled = true;

    /// <summary>An attached animator keeps its flag in the world; one made with <c>new</c> holds it.</summary>
    private protected override bool EnabledState
    {
        get => Handle != 0 ? NativeBehaviourState.Read(Handle) : _enabled;
        set
        {
            _enabled = value;
            if (Handle != 0) NativeBehaviourState.Write(Handle, value);
        }
    }

    /// <summary>Copies the managed settings of the animator this one was cloned from; the world copies the enabled flag.
    /// <paramref name="remap"/> maps an object of the original hierarchy to its clone.</summary>
    internal void CopyManagedState(Animator source, Func<Object, Object> remap)
    {
        runtimeAnimatorController = (RuntimeAnimatorController?)MappedOrNull(source.runtimeAnimatorController, remap);
        avatar = (Avatar?)MappedOrNull(source.avatar, remap);
        speed = source.speed;
        cullingMode = source.cullingMode;
        updateMode = source.updateMode;
        applyRootMotion = source.applyRootMotion;
        linearVelocityBlending = source.linearVelocityBlending;
        stabilizeFeet = source.stabilizeFeet;
        layersAffectMassCenter = source.layersAffectMassCenter;
        feetPivotActive = source.feetPivotActive;
        logWarnings = source.logWarnings;
        fireEvents = source.fireEvents;
        keepAnimatorStateOnDisable = source.keepAnimatorStateOnDisable;
        writeDefaultValuesOnDisable = source.writeDefaultValuesOnDisable;
    }

    private static Object? MappedOrNull(Object? value, Func<Object, Object> remap) => value is null ? null : remap(value);
}
