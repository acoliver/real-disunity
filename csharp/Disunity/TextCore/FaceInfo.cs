namespace UnityEngine.TextCore;

/// <summary>
/// The measurements of a font face a font asset stores, in the font's point size. The serialized
/// fields carry the names a player stores them under, so loaded data binds to them.
/// </summary>
[Serializable]
public struct FaceInfo
{
    [SerializeField] private int m_FaceIndex;
    [SerializeField] private string m_FamilyName;
    [SerializeField] private string m_StyleName;
    [SerializeField] private int m_PointSize;
    [SerializeField] private float m_Scale;
    [SerializeField] private int m_UnitsPerEM;
    [SerializeField] private float m_LineHeight;
    [SerializeField] private float m_AscentLine;
    [SerializeField] private float m_CapLine;
    [SerializeField] private float m_MeanLine;
    [SerializeField] private float m_Baseline;
    [SerializeField] private float m_DescentLine;
    [SerializeField] private float m_SuperscriptOffset;
    [SerializeField] private float m_SuperscriptSize;
    [SerializeField] private float m_SubscriptOffset;
    [SerializeField] private float m_SubscriptSize;
    [SerializeField] private float m_UnderlineOffset;
    [SerializeField] private float m_UnderlineThickness;
    [SerializeField] private float m_StrikethroughOffset;
    [SerializeField] private float m_StrikethroughThickness;
    [SerializeField] private float m_TabWidth;
    public int faceIndex { get => m_FaceIndex; set => m_FaceIndex = value; }
    public string familyName { get => m_FamilyName; set => m_FamilyName = value; }
    public string styleName { get => m_StyleName; set => m_StyleName = value; }
    public int pointSize { get => m_PointSize; set => m_PointSize = value; }
    public float scale { get => m_Scale; set => m_Scale = value; }
    public int unitsPerEM { get => m_UnitsPerEM; set => m_UnitsPerEM = value; }
    public float lineHeight { get => m_LineHeight; set => m_LineHeight = value; }
    public float ascentLine { get => m_AscentLine; set => m_AscentLine = value; }
    public float capLine { get => m_CapLine; set => m_CapLine = value; }
    public float meanLine { get => m_MeanLine; set => m_MeanLine = value; }
    public float baseline { get => m_Baseline; set => m_Baseline = value; }
    public float descentLine { get => m_DescentLine; set => m_DescentLine = value; }
    public float superscriptOffset { get => m_SuperscriptOffset; set => m_SuperscriptOffset = value; }
    public float superscriptSize { get => m_SuperscriptSize; set => m_SuperscriptSize = value; }
    public float subscriptOffset { get => m_SubscriptOffset; set => m_SubscriptOffset = value; }
    public float subscriptSize { get => m_SubscriptSize; set => m_SubscriptSize = value; }
    public float underlineOffset { get => m_UnderlineOffset; set => m_UnderlineOffset = value; }
    public float underlineThickness { get => m_UnderlineThickness; set => m_UnderlineThickness = value; }
    public float strikethroughOffset { get => m_StrikethroughOffset; set => m_StrikethroughOffset = value; }
    public float strikethroughThickness { get => m_StrikethroughThickness; set => m_StrikethroughThickness = value; }
    public float tabWidth { get => m_TabWidth; set => m_TabWidth = value; }
}
