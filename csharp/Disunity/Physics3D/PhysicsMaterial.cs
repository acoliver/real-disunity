namespace UnityEngine;

/// <summary>Surface friction and bounce for 3D colliders, stored with Unity's defaults. No simulation reads it.</summary>
public class PhysicsMaterial : Object
{
    public PhysicsMaterial() { }

    public PhysicsMaterial(string name) => this.name = name;

    public float bounciness { get; set; }
    public float dynamicFriction { get; set; } = 0.6f;
    public float staticFriction { get; set; } = 0.6f;
    public PhysicsMaterialCombine frictionCombine { get; set; } = PhysicsMaterialCombine.Average;
    public PhysicsMaterialCombine bounceCombine { get; set; } = PhysicsMaterialCombine.Average;
}
