using Unity.Collections;

namespace UnityEngine;

/// <summary>One contact point of a <see cref="ContactPair"/>. The engine raises no 3D contact events.</summary>
public readonly struct ContactPairPoint
{
    private readonly Vector3 m_Position;
    private readonly float m_Separation;
    private readonly Vector3 m_Normal;
    private readonly uint m_InternalFaceIndex0;
    private readonly Vector3 m_Impulse;
    private readonly uint m_InternalFaceIndex1;

    internal ContactPairPoint(Vector3 position, float separation, Vector3 normal, Vector3 impulse, (uint first, uint second) faces)
    {
        m_Position = position;
        m_Separation = separation;
        m_Normal = normal;
        m_Impulse = impulse;
        m_InternalFaceIndex0 = faces.first;
        m_InternalFaceIndex1 = faces.second;
    }

    internal uint FaceIndex(bool second) => second ? m_InternalFaceIndex1 : m_InternalFaceIndex0;

    public Vector3 position => m_Position;
    public float separation => m_Separation;
    public Vector3 normal => m_Normal;
    public Vector3 impulse => m_Impulse;
    public Vector3 Position => m_Position;
    public float Separation => m_Separation;
    public Vector3 Normal => m_Normal;
    public Vector3 Impulse => m_Impulse;
}

/// <summary>Two colliders in contact, with their contact points.</summary>
public readonly struct ContactPair
{
    private readonly EntityId m_ColliderID;
    private readonly EntityId m_OtherColliderID;
    private readonly ContactPairPoint[]? m_Points;
    private readonly byte m_Phase;

    internal ContactPair(EntityId collider, EntityId otherCollider, ContactPairPoint[] points, byte phase)
    {
        m_ColliderID = collider;
        m_OtherColliderID = otherCollider;
        m_Points = points;
        m_Phase = phase;
    }

    private ContactPairPoint[] Points => m_Points ?? [];

    public int colliderInstanceID => m_ColliderID;
    public int otherColliderInstanceID => m_OtherColliderID;
    public EntityId colliderEntityId => m_ColliderID;
    public EntityId otherColliderEntityId => m_OtherColliderID;
    public Collider? collider => null;
    public Collider? otherCollider => null;
    public int contactCount => Points.Length;
    public Vector3 impulseSum => Points.Aggregate(Vector3.zero, (sum, point) => sum + point.impulse);
    public bool isCollisionEnter => m_Phase == 0;
    public bool isCollisionExit => m_Phase == 2;
    public bool isCollisionStay => m_Phase == 1;
    public int ColliderInstanceID => m_ColliderID;
    public int OtherColliderInstanceID => m_OtherColliderID;
    public Collider? Collider => null;
    public Collider? OtherCollider => null;
    public int ContactCount => Points.Length;
    public Vector3 ImpulseSum => impulseSum;
    public bool IsCollisionEnter => isCollisionEnter;
    public bool IsCollisionExit => isCollisionExit;
    public bool IsCollisionStay => isCollisionStay;

    public void CopyToNativeArray(NativeArray<ContactPairPoint> buffer)
    {
        if (buffer.Length < Points.Length) throw new ArgumentException("The buffer is smaller than the contact count", nameof(buffer));
        for (var i = 0; i < Points.Length; i++) buffer[i] = Points[i];
    }

    public ref readonly ContactPairPoint GetContactPoint(int index) => ref Points[index];

    public uint GetContactPointFaceIndex(int contactIndex) => Points[contactIndex].FaceIndex(false);
}

/// <summary>The contact pairs between two bodies in one step.</summary>
public readonly struct ContactPairHeader
{
    private readonly EntityId m_BodyID;
    private readonly EntityId m_OtherBodyID;
    private readonly ContactPair[]? m_Pairs;
    private readonly (Vector3 linear, Vector3 angular) m_ThisBody;
    private readonly (Vector3 linear, Vector3 angular) m_OtherBody;

    internal ContactPairHeader((EntityId body, EntityId other) bodies, ContactPair[] pairs,
        (Vector3 linear, Vector3 angular) thisBody, (Vector3 linear, Vector3 angular) otherBody)
    {
        m_BodyID = bodies.body;
        m_OtherBodyID = bodies.other;
        m_Pairs = pairs;
        m_ThisBody = thisBody;
        m_OtherBody = otherBody;
    }

    private ContactPair[] Pairs => m_Pairs ?? [];

    public int bodyInstanceID => m_BodyID;
    public int otherBodyInstanceID => m_OtherBodyID;
    public EntityId bodyEntityId => m_BodyID;
    public EntityId otherBodyEntityId => m_OtherBodyID;
    public Component? body => null;
    public Component? otherBody => null;
    public Vector3 bodyLinearVelocity => m_ThisBody.linear;
    public Vector3 bodyAngularVelocity => m_ThisBody.angular;
    public Vector3 otherBodyLinearVelocity => m_OtherBody.linear;
    public Vector3 otherBodyAngularVelocity => m_OtherBody.angular;
    public int pairCount => Pairs.Length;
    public int BodyInstanceID => m_BodyID;
    public int OtherBodyInstanceID => m_OtherBodyID;
    public Component? Body => null;
    public Component? OtherBody => null;
    public int PairCount => Pairs.Length;

    public ref readonly ContactPair GetContactPair(int index) => ref Pairs[index];
}
