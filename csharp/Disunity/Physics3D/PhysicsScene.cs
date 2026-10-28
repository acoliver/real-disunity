namespace UnityEngine;

/// <summary>
/// A 3D physics scene. The engine has one, the default scene, and it holds no colliders or bodies, so
/// every query on it gives the empty-world answer (see <see cref="Physics"/>).
/// </summary>
public partial struct PhysicsScene : IEquatable<PhysicsScene>
{
    internal const int DefaultHandle = 1;

    private readonly int m_Handle;

    internal PhysicsScene(int handle) => m_Handle = handle;

    public static bool operator ==(PhysicsScene lhs, PhysicsScene rhs) => lhs.m_Handle == rhs.m_Handle;
    public static bool operator !=(PhysicsScene lhs, PhysicsScene rhs) => lhs.m_Handle != rhs.m_Handle;

    public override readonly string ToString() => $"({m_Handle})";
    public override readonly int GetHashCode() => m_Handle;
    public override readonly bool Equals(object? other) => other is PhysicsScene scene && this == scene;
    public readonly bool Equals(PhysicsScene other) => this == other;

    public readonly bool IsValid() => m_Handle != 0;

    public readonly bool IsEmpty() =>
        IsValid() ? true : throw new InvalidOperationException("Cannot check if physics scene is empty as it is invalid.");

    /// <summary>Nothing to advance: the scene has no bodies, so stepping it changes nothing observable.</summary>
    public readonly void Simulate(float step) { }
    public readonly void RunSimulationStages(float step, SimulationStage stages, SimulationOption options) { }
    public readonly void ReleaseLastSimulationStepBuffers() { }
    public readonly void InterpolateBodies() { }
    public readonly void ResetInterpolationPoses() { }
}
