namespace UnityEngine;

/// <summary>
/// The components the engine cannot attach to a GameObject, because it has nothing to run them:
/// no 3D physics simulation owns colliders and rigidbodies, no animation playback owns
/// Animators, and no audio output plays AudioSources. Instances built with <c>new</c> hold stored state and never join a world.
/// </summary>
internal static class UnsupportedComponents
{
    /// <summary>Throws for a component the engine cannot build; any other component passes.</summary>
    internal static void Reject(Component component)
    {
        if (component is Collider or Rigidbody or ArticulationBody)
            throw new NotSupportedException($"3D physics components are not supported yet: {component.GetType().Name}");
        if (component is LightProbeProxyVolume)
            throw new NotSupportedException("LightProbeProxyVolume components are not supported yet: the engine has no probe lighting");
        if (component is Animator)
            throw new NotSupportedException("Animator components are not supported yet");
        if (component is AudioSource)
            throw new NotSupportedException("AudioSource components are not supported yet: the engine has no audio output");
    }
}
