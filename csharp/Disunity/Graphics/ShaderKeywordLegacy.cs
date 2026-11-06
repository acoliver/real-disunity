namespace UnityEngine.Rendering;

/// <summary>
/// A global shader keyword by name, the older form of <see cref="GlobalKeyword"/>. The engine keeps keywords by name only, so
/// the struct holds the name; <see cref="index"/> is the name's registration order among the names made this way and
/// <see cref="IsValid"/> is true when the name is not empty. Equal when the names are equal.
/// </summary>
public readonly struct ShaderKeyword : IEquatable<ShaderKeyword>
{
    private static readonly List<string> Names = new();
    private readonly string? _name;

    public ShaderKeyword(string keywordName)
    {
        ArgumentNullException.ThrowIfNull(keywordName);
        _name = keywordName;
        lock (Names)
            if (!Names.Contains(keywordName)) Names.Add(keywordName);
    }

    public string name => _name ?? "";
    public bool IsValid() => !string.IsNullOrEmpty(_name);

    public int index
    {
        get
        {
            lock (Names) return _name is null ? -1 : Names.IndexOf(_name);
        }
    }

    public ShaderKeywordType GetKeywordType() => IsValid() ? ShaderKeywordType.UserDefined : ShaderKeywordType.None;

    public override string ToString() => name;
    public bool Equals(ShaderKeyword other) => name == other.name;
    public override bool Equals(object? obj) => obj is ShaderKeyword other && Equals(other);
    public override int GetHashCode() => name.GetHashCode();
    public static bool operator ==(ShaderKeyword left, ShaderKeyword right) => left.Equals(right);
    public static bool operator !=(ShaderKeyword left, ShaderKeyword right) => !left.Equals(right);
}
