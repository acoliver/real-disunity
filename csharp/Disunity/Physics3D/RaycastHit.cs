namespace UnityEngine;

/// <summary>
/// The result of a 3D physics query. The engine has no 3D colliders, so no query fills one in and
/// <see cref="collider"/> and everything derived from it are null.
/// </summary>
public struct RaycastHit
{
    private Vector3 m_Point;
    private Vector3 m_Normal;
    private readonly uint m_FaceID;
    private float m_Distance;
    private Vector2 m_UV;
    private readonly EntityId m_Collider;

    internal RaycastHit(Vector3 point, Vector3 normal, float distance, uint faceId, Vector2 uv, EntityId collider)
    {
        m_Point = point;
        m_Normal = normal;
        m_Distance = distance;
        m_FaceID = faceId;
        m_UV = uv;
        m_Collider = collider;
    }

    public readonly Collider? collider => null;
    public readonly int colliderInstanceID => m_Collider;
    public readonly EntityId colliderEntityId => m_Collider;

    public Vector3 point { readonly get => m_Point; set => m_Point = value; }
    public Vector3 normal { readonly get => m_Normal; set => m_Normal = value; }
    public float distance { readonly get => m_Distance; set => m_Distance = value; }

    /// <summary>Weights of the hit triangle's three corners, kept in the two texture coordinate slots as Unity does.</summary>
    public Vector3 barycentricCoordinate
    {
        readonly get => new(1f - m_UV.x - m_UV.y, m_UV.x, m_UV.y);
        set => m_UV = new Vector2(value.y, value.z);
    }

    public readonly int triangleIndex => (int)m_FaceID;
    public readonly Vector2 textureCoord => m_UV;
    public readonly Vector2 textureCoord2 => Vector2.zero;
    public readonly Vector2 lightmapCoord => Vector2.zero;
    public readonly Transform? transform => collider?.transform;
    public readonly Rigidbody? rigidbody => collider?.attachedRigidbody;
    public readonly ArticulationBody? articulationBody => collider?.attachedArticulationBody;
}
