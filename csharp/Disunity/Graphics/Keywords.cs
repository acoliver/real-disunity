namespace UnityEngine.Rendering;

/// <summary>A shader keyword that is enabled for every shader. The engine keeps the set in <see cref="Shader"/>.</summary>
public readonly struct GlobalKeyword
{
    private readonly string? m_Name;

    public GlobalKeyword(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        m_Name = name;
        Shader.NoteGlobalKeyword(name);
    }

    public static GlobalKeyword Create(string name) => new(name);

    public string name => m_Name ?? "";

    public override string ToString() => name;
}

/// <summary>
/// A keyword of one shader. The engine has no shader keyword tables, so a keyword is valid when it has a
/// name, it is a user-defined keyword, and it is neither dynamic nor overridable. Two keywords are equal
/// when they have the same name and belong to the same shader.
/// </summary>
public readonly struct LocalKeyword : IEquatable<LocalKeyword>
{
    private readonly Shader? m_Shader;
    private readonly string? m_Name;

    public LocalKeyword(Shader shader, string name)
    {
        ArgumentNullException.ThrowIfNull(shader);
        ArgumentNullException.ThrowIfNull(name);
        m_Shader = shader;
        m_Name = name;
    }

    public string name => m_Name ?? "";
    public bool isDynamic => false;
    public bool isOverridable => false;
    public bool isValid => !string.IsNullOrEmpty(m_Name);
    public ShaderKeywordType type => isValid ? ShaderKeywordType.UserDefined : ShaderKeywordType.None;

    public override string ToString() => name;
    public override bool Equals(object? o) => o is LocalKeyword other && Equals(other);
    public bool Equals(LocalKeyword rhs) => m_Name == rhs.m_Name && ReferenceEquals(m_Shader, rhs.m_Shader);
    public override int GetHashCode() => HashCode.Combine(m_Name, m_Shader is null ? 0 : System.Runtime.CompilerServices.RuntimeHelpers.GetHashCode(m_Shader));
    public static bool operator ==(LocalKeyword lhs, LocalKeyword rhs) => lhs.Equals(rhs);
    public static bool operator !=(LocalKeyword lhs, LocalKeyword rhs) => !lhs.Equals(rhs);
}

/// <summary>The keywords a shader declares. A shader's keyword table is not available, so the lists throw
/// <see cref="NotSupportedException"/>; a Shader hands out no space, so only a default space exists.</summary>
public readonly struct LocalKeywordSpace : IEquatable<LocalKeywordSpace>
{
    public LocalKeyword[] keywords => throw Unsupported(nameof(keywords));
    public string[] keywordNames => throw Unsupported(nameof(keywordNames));
    public uint keywordCount => throw Unsupported(nameof(keywordCount));
    public LocalKeyword FindKeyword(string name) => throw Unsupported(nameof(FindKeyword));

    public override bool Equals(object? o) => o is LocalKeywordSpace;
    public bool Equals(LocalKeywordSpace rhs) => true;
    public override int GetHashCode() => 0;
    public static bool operator ==(LocalKeywordSpace lhs, LocalKeywordSpace rhs) => true;
    public static bool operator !=(LocalKeywordSpace lhs, LocalKeywordSpace rhs) => false;

    private static NotSupportedException Unsupported(string member) => new($"LocalKeywordSpace.{member} is not supported yet: the engine has no shader keyword tables");
}

/// <summary>The second-order spherical harmonics of a light probe: 9 coefficients for each of red, green and blue.</summary>
public unsafe struct SphericalHarmonicsL2 : IEquatable<SphericalHarmonicsL2>
{
    private const int Count = 27;

    private fixed float m_Coefficients[Count];

    private static int Slot(int rgb, int coefficient)
    {
        if ((uint)rgb > 2 || (uint)coefficient > 8) throw new IndexOutOfRangeException("SphericalHarmonicsL2 index must be 0 to 2 and 0 to 8");
        return rgb * 9 + coefficient;
    }

    public float this[int rgb, int coefficient]
    {
        readonly get => m_Coefficients[Slot(rgb, coefficient)];
        set => m_Coefficients[Slot(rgb, coefficient)] = value;
    }

    public void Clear() => this = default;

    /// <summary>Adding lights needs Unity's basis projection constants, which the engine does not have yet.</summary>
    public void AddAmbientLight(Color color) => throw Unsupported(nameof(AddAmbientLight));

    public void AddAmbientLight(ref Color color) => throw Unsupported(nameof(AddAmbientLight));

    public void AddDirectionalLight(Vector3 direction, Color color, float intensity) => throw Unsupported(nameof(AddDirectionalLight));

    public void AddDirectionalLight(ref Vector3 direction, ref Color color, float intensity) => throw Unsupported(nameof(AddDirectionalLight));

    public void Evaluate(Vector3[] directions, Color[] results) => throw Unsupported(nameof(Evaluate));

    private static NotSupportedException Unsupported(string member) => new($"SphericalHarmonicsL2.{member} is not supported yet");

    public override readonly int GetHashCode()
    {
        var hash = new HashCode();
        for (var i = 0; i < Count; i++) hash.Add(m_Coefficients[i]);
        return hash.ToHashCode();
    }

    public override readonly bool Equals(object? other) => other is SphericalHarmonicsL2 value && Equals(value);

    public readonly bool Equals(SphericalHarmonicsL2 other) => Equals(ref other);

    public readonly bool Equals(ref SphericalHarmonicsL2 other)
    {
        for (var i = 0; i < Count; i++)
            if (m_Coefficients[i] != other.m_Coefficients[i]) return false;
        return true;
    }

    public static SphericalHarmonicsL2 operator *(SphericalHarmonicsL2 lhs, float rhs) => Combine(lhs, default, rhs, 0);
    public static SphericalHarmonicsL2 operator *(float lhs, SphericalHarmonicsL2 rhs) => Combine(rhs, default, lhs, 0);
    public static SphericalHarmonicsL2 operator +(SphericalHarmonicsL2 lhs, SphericalHarmonicsL2 rhs) => Combine(lhs, rhs, 1, 1);
    public static bool operator ==(SphericalHarmonicsL2 lhs, SphericalHarmonicsL2 rhs) => lhs.Equals(ref rhs);
    public static bool operator !=(SphericalHarmonicsL2 lhs, SphericalHarmonicsL2 rhs) => !lhs.Equals(ref rhs);

    private static SphericalHarmonicsL2 Combine(SphericalHarmonicsL2 a, SphericalHarmonicsL2 b, float weightA, float weightB)
    {
        var result = new SphericalHarmonicsL2();
        for (var i = 0; i < Count; i++) result.m_Coefficients[i] = a.m_Coefficients[i] * weightA + b.m_Coefficients[i] * weightB;
        return result;
    }
}
