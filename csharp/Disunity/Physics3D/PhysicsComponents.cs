namespace UnityEngine;

/// <summary>Which components the engine cannot attach, because it has no 3D physics simulation to own them.</summary>
internal static class PhysicsComponents
{
    /// <summary>Throws for 3D colliders, rigidbodies and articulation bodies; any other component passes.</summary>
    internal static void RejectUnsupported(Component component)
    {
        if (component is Collider or Rigidbody or ArticulationBody)
            throw new NotSupportedException($"3D physics components are not supported yet: {component.GetType().Name}");
    }
}
