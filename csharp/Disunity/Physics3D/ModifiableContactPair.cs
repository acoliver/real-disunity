namespace UnityEngine;

/// <summary>
/// A contact pair a contact-modification callback may edit. The engine raises no 3D contact
/// callbacks, so only code that builds one itself (through the internal constructor) sees contacts.
/// </summary>
public struct ModifiableContactPair
{
    internal sealed class Contact
    {
        internal Vector3 Point, Normal, TargetVelocity;
        internal float Separation, Bounciness, StaticFriction, DynamicFriction, MaxImpulse;
        internal readonly uint FaceIndex;

        internal Contact(uint faceIndex) => FaceIndex = faceIndex;
    }

    public Quaternion rotation;
    public Vector3 position;
    public Quaternion otherRotation;
    public Vector3 otherPosition;

    private readonly Contact[]? m_Contacts;
    private readonly (int collider, int body) m_Self;
    private readonly (int collider, int body) m_Other;
    private readonly (Vector3 linear, Vector3 angular) m_SelfVelocity;
    private readonly (Vector3 linear, Vector3 angular) m_OtherVelocity;

    internal ModifiableContactPair(Contact[] contacts, (int collider, int body) self, (int collider, int body) other)
    {
        rotation = Quaternion.identity;
        position = Vector3.zero;
        otherRotation = Quaternion.identity;
        otherPosition = Vector3.zero;
        massProperties = default;
        m_Contacts = contacts;
        m_Self = self;
        m_Other = other;
        m_SelfVelocity = default;
        m_OtherVelocity = default;
    }

    private readonly Contact At(int i) =>
        m_Contacts is { } contacts && (uint)i < (uint)contacts.Length ? contacts[i] : throw new ArgumentOutOfRangeException(nameof(i));

    public readonly int colliderInstanceID => m_Self.collider;
    public readonly int otherColliderInstanceID => m_Other.collider;
    public readonly int bodyInstanceID => m_Self.body;
    public readonly int otherBodyInstanceID => m_Other.body;
    public readonly Vector3 bodyVelocity => m_SelfVelocity.linear;
    public readonly Vector3 bodyAngularVelocity => m_SelfVelocity.angular;
    public readonly Vector3 otherBodyVelocity => m_OtherVelocity.linear;
    public readonly Vector3 otherBodyAngularVelocity => m_OtherVelocity.angular;
    public readonly int contactCount => m_Contacts?.Length ?? 0;
    public ModifiableMassProperties massProperties { readonly get; set; }

    public readonly Vector3 GetPoint(int i) => At(i).Point;
    public readonly void SetPoint(int i, Vector3 v) => At(i).Point = v;
    public readonly Vector3 GetNormal(int i) => At(i).Normal;
    public readonly void SetNormal(int i, Vector3 normal) => At(i).Normal = normal;
    public readonly float GetSeparation(int i) => At(i).Separation;
    public readonly void SetSeparation(int i, float separation) => At(i).Separation = separation;
    public readonly Vector3 GetTargetVelocity(int i) => At(i).TargetVelocity;
    public readonly void SetTargetVelocity(int i, Vector3 velocity) => At(i).TargetVelocity = velocity;
    public readonly float GetBounciness(int i) => At(i).Bounciness;
    public readonly void SetBounciness(int i, float bounciness) => At(i).Bounciness = bounciness;
    public readonly float GetStaticFriction(int i) => At(i).StaticFriction;
    public readonly void SetStaticFriction(int i, float staticFriction) => At(i).StaticFriction = staticFriction;
    public readonly float GetDynamicFriction(int i) => At(i).DynamicFriction;
    public readonly void SetDynamicFriction(int i, float dynamicFriction) => At(i).DynamicFriction = dynamicFriction;
    public readonly float GetMaxImpulse(int i) => At(i).MaxImpulse;
    public readonly void SetMaxImpulse(int i, float value) => At(i).MaxImpulse = value;
    public readonly uint GetFaceIndex(int i) => At(i).FaceIndex;

    /// <summary>Makes the contact have no effect: zero max impulse.</summary>
    public readonly void IgnoreContact(int i) => At(i).MaxImpulse = 0f;
}
