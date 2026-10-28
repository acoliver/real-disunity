namespace UnityEngine;

/// <summary>Describes the physics integration in use. The engine has none for 3D, so this names that fact.</summary>
public readonly struct IntegrationInfo
{
    private readonly uint m_Id;
    private readonly string? m_Name;
    private readonly string? m_Description;
    private readonly bool m_IsFallback;

    internal IntegrationInfo(uint id, string name, string description, bool isFallback)
    {
        m_Id = id;
        m_Name = name;
        m_Description = description;
        m_IsFallback = isFallback;
    }

    public uint id => m_Id;
    public string name => m_Name ?? "";
    public string description => m_Description ?? "";
    public bool isFallback => m_IsFallback;
}

public struct ModifiableMassProperties
{
    public float inverseMassScale;
    public float inverseInertiaScale;
    public float otherInverseMassScale;
    public float otherInverseInertiaScale;
}
