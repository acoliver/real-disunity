namespace UnityEngine;

[AttributeUsage(AttributeTargets.Field)]
public sealed class HeaderAttribute(string header) : Attribute
{
    public string header { get; } = header;
}

[AttributeUsage(AttributeTargets.Field)]
public sealed class RangeAttribute(float min, float max) : Attribute
{
    public float min { get; } = min;
    public float max { get; } = max;
}

[AttributeUsage(AttributeTargets.Class)]
public sealed class CreateAssetMenuAttribute : Attribute
{
    public string menuName { get; set; } = "";
    public string fileName { get; set; } = "";
    public int order { get; set; }
}
