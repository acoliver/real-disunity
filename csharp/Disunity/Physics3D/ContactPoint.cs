namespace UnityEngine;

/// <summary>One contact of a 3D collision. Its colliders are identified by entity ID; the engine produces no 3D contacts.</summary>
public readonly struct ContactPoint
{
    private readonly Vector3 m_Point;
    private readonly Vector3 m_Normal;
    private readonly Vector3 m_Impulse;
    private readonly EntityId m_ThisColliderEntityId;
    private readonly EntityId m_OtherColliderEntityId;
    private readonly float m_Separation;

    internal ContactPoint(Vector3 point, Vector3 normal, Vector3 impulse, float separation, (EntityId self, EntityId other) colliders)
    {
        m_Point = point;
        m_Normal = normal;
        m_Impulse = impulse;
        m_Separation = separation;
        m_ThisColliderEntityId = colliders.self;
        m_OtherColliderEntityId = colliders.other;
    }

    public Vector3 point => m_Point;
    public Vector3 normal => m_Normal;
    public Vector3 impulse => m_Impulse;
    public Collider? thisCollider => null;
    public Collider? otherCollider => null;
    public float separation => m_Separation;
}
