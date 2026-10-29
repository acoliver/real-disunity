namespace UnityEngine;

/// <summary>
/// A 3D articulation body. Only the type exists, so collisions, bodies and queries can refer to it;
/// it cannot be added to a GameObject and its members are not provided.
/// </summary>
public class ArticulationBody : Behaviour
{
    private protected override bool EnabledState { get; set; } = true;
}
