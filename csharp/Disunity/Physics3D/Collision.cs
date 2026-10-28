namespace UnityEngine;

/// <summary>
/// What a 3D collision callback receives. The engine raises no 3D collision callbacks; a collision
/// built with the public constructor has no colliders, bodies or contacts.
/// </summary>
public class Collision
{
    private readonly Collider? m_Self;
    private readonly Collider? m_Other;
    private readonly ContactPoint[] m_Contacts;
    private readonly Vector3 m_Impulse;
    private readonly Vector3 m_RelativeVelocity;

    public Collision() : this(null, null, [], Vector3.zero, Vector3.zero) { }

    internal Collision(Collider? self, Collider? other, ContactPoint[] contacts, Vector3 impulse, Vector3 relativeVelocity)
    {
        m_Self = self;
        m_Other = other;
        m_Contacts = contacts;
        m_Impulse = impulse;
        m_RelativeVelocity = relativeVelocity;
    }

    public Vector3 impulse => m_Impulse;
    public Vector3 relativeVelocity => m_RelativeVelocity;
    public Collider? thisCollider => m_Self;
    public Collider? collider => m_Other;
    public Rigidbody? thisRigidbody => m_Self?.attachedRigidbody;
    public Rigidbody? rigidbody => m_Other?.attachedRigidbody;
    public ArticulationBody? thisArticulationBody => m_Self?.attachedArticulationBody;
    public ArticulationBody? articulationBody => m_Other?.attachedArticulationBody;
    public Component? thisBody => (Component?)thisRigidbody ?? thisArticulationBody;
    public Component? body => (Component?)rigidbody ?? articulationBody;
    public Vector3 thisLinearVelocity => thisRigidbody?.linearVelocity ?? Vector3.zero;
    public Vector3 linearVelocity => rigidbody?.linearVelocity ?? Vector3.zero;
    public Vector3 thisAngularVelocity => thisRigidbody?.angularVelocity ?? Vector3.zero;
    public Vector3 angularVelocity => rigidbody?.angularVelocity ?? Vector3.zero;
    public Transform? thisTransform => m_Self?.gameObject?.transform;
    public Transform? transform => m_Other?.gameObject?.transform;
    public GameObject? thisGameObject => m_Self?.gameObject;
    public GameObject? gameObject => m_Other?.gameObject;
    public int contactCount => m_Contacts.Length;
    public ContactPoint[] contacts => (ContactPoint[])m_Contacts.Clone();

    public ContactPoint GetContact(int index) =>
        (uint)index < (uint)m_Contacts.Length ? m_Contacts[index] : throw new ArgumentOutOfRangeException(nameof(index));

    public int GetContacts(ContactPoint[] contacts)
    {
        ArgumentNullException.ThrowIfNull(contacts);
        var count = Math.Min(contacts.Length, m_Contacts.Length);
        Array.Copy(m_Contacts, contacts, count);
        return count;
    }

    public int GetContacts(List<ContactPoint> contacts)
    {
        ArgumentNullException.ThrowIfNull(contacts);
        contacts.Clear();
        contacts.AddRange(m_Contacts);
        return contacts.Count;
    }
}
