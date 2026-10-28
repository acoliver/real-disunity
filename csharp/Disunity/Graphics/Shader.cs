using Disunity.Hosting;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A shader known to the engine by name. A shader exists once <see cref="DisunityHost.RegisterShader"/> has
/// registered it, and <see cref="Find"/> returns only registered shaders. The engine has no shader programs
/// or declarations, so the members that read a shader's passes, tags, keywords or properties throw
/// <see cref="NotSupportedException"/> (a shader reports one subshader of one pass). The global properties and
/// keywords are stored and read back, and nothing applies them to a draw yet.
/// </summary>
public sealed partial class Shader : Object
{
    private static readonly NameTable Properties = new(1000);
    private static readonly List<string> KnownKeywords = new();
    private static readonly List<string> EnabledKeywords = new();

    internal Shader(string name) => this.name = name;

    public static ShaderHardwareTier globalShaderHardwareTier { get; set; } = ShaderHardwareTier.Tier3;
    public static int maximumChunksOverride { get; set; }
    public static int globalMaximumLOD { get; set; } = int.MaxValue;
    public static string globalRenderPipeline { get; set; } = "";
    public int maximumLOD { get; set; }
    public bool isSupported => !Destroyed;
    public int renderQueue => 2000;
    public int passCount => 1;
    public int subshaderCount => 1;

    public static Shader? Find(string name) => DisunityHost.FindShader(name);

    /// <summary>The integer for a property name: the same for the same name in this process, and a different one
    /// for a different name. The numbers start at 1000 and are not Unity's.</summary>
    public static int PropertyToID(string name) => Properties.IdOf(name);

    /// <summary>The name of a property ID this process handed out, or null.</summary>
    internal static string? PropertyName(int nameID) => Properties.NameOf(nameID);

    /// <summary>Unity compiles every shader variant ahead of time here; the engine has none to compile.</summary>
    public static void WarmupAllShaders() { }

    internal static void NoteGlobalKeyword(string name)
    {
        if (!KnownKeywords.Contains(name)) KnownKeywords.Add(name);
    }

    public static GlobalKeyword[] globalKeywords => KnownKeywords.Select(keyword => new GlobalKeyword(keyword)).ToArray();

    public static GlobalKeyword[] enabledGlobalKeywords => EnabledKeywords.Select(keyword => new GlobalKeyword(keyword)).ToArray();

    public static void EnableKeyword(string keyword) => SetGlobalKeyword(keyword, true);

    public static void DisableKeyword(string keyword) => SetGlobalKeyword(keyword, false);

    public static bool IsKeywordEnabled(string keyword)
    {
        ArgumentNullException.ThrowIfNull(keyword);
        return EnabledKeywords.Contains(keyword);
    }

    public static void EnableKeyword(ref GlobalKeyword keyword) => SetGlobalKeyword(keyword.name, true);

    public static void DisableKeyword(ref GlobalKeyword keyword) => SetGlobalKeyword(keyword.name, false);

    public static void SetKeyword(ref GlobalKeyword keyword, bool value) => SetGlobalKeyword(keyword.name, value);

    public static bool IsKeywordEnabled(ref GlobalKeyword keyword) => IsKeywordEnabled(keyword.name);

    private static void SetGlobalKeyword(string keyword, bool enabled)
    {
        ArgumentNullException.ThrowIfNull(keyword);
        NoteGlobalKeyword(keyword);
        if (enabled && !EnabledKeywords.Contains(keyword)) EnabledKeywords.Add(keyword);
        else if (!enabled) EnabledKeywords.Remove(keyword);
    }

    public int GetPassCountInSubshader(int subshaderIndex) =>
        subshaderIndex == 0 ? passCount : throw new ArgumentOutOfRangeException(nameof(subshaderIndex), "a shader has one subshader");

    public LocalKeywordSpace keywordSpace => throw Unsupported(nameof(keywordSpace));

    public Shader GetDependency(string name) => throw Unsupported(nameof(GetDependency));

    public ShaderTagId FindPassTagValue(int passIndex, ShaderTagId tagName) => throw Unsupported(nameof(FindPassTagValue));

    public ShaderTagId FindPassTagValue(int subshaderIndex, int passIndex, ShaderTagId tagName) => throw Unsupported(nameof(FindPassTagValue));

    public ShaderTagId FindSubshaderTagValue(int subshaderIndex, ShaderTagId tagName) => throw Unsupported(nameof(FindSubshaderTagValue));

    public int GetPropertyCount() => throw Unsupported(nameof(GetPropertyCount));

    public int FindPropertyIndex(string propertyName) => throw Unsupported(nameof(FindPropertyIndex));

    public string GetPropertyName(int propertyIndex) => throw Unsupported(nameof(GetPropertyName));

    public int GetPropertyNameId(int propertyIndex) => throw Unsupported(nameof(GetPropertyNameId));

    public ShaderPropertyType GetPropertyType(int propertyIndex) => throw Unsupported(nameof(GetPropertyType));

    public string GetPropertyDescription(int propertyIndex) => throw Unsupported(nameof(GetPropertyDescription));

    public ShaderPropertyFlags GetPropertyFlags(int propertyIndex) => throw Unsupported(nameof(GetPropertyFlags));

    public string[] GetPropertyAttributes(int propertyIndex) => throw Unsupported(nameof(GetPropertyAttributes));

    public float GetPropertyDefaultFloatValue(int propertyIndex) => throw Unsupported(nameof(GetPropertyDefaultFloatValue));

    public Vector4 GetPropertyDefaultVectorValue(int propertyIndex) => throw Unsupported(nameof(GetPropertyDefaultVectorValue));

    public Vector2 GetPropertyRangeLimits(int propertyIndex) => throw Unsupported(nameof(GetPropertyRangeLimits));

    public int GetPropertyDefaultIntValue(int propertyIndex) => throw Unsupported(nameof(GetPropertyDefaultIntValue));

    public TextureDimension GetPropertyTextureDimension(int propertyIndex) => throw Unsupported(nameof(GetPropertyTextureDimension));

    public string GetPropertyTextureDefaultName(int propertyIndex) => throw Unsupported(nameof(GetPropertyTextureDefaultName));

    public bool FindTextureStack(int propertyIndex, ref string stackName, ref int layerIndex) => throw Unsupported(nameof(FindTextureStack));

    private static NotSupportedException Unsupported(string member) =>
        new($"Shader.{member} is not supported yet: the engine has no shader declarations");
}
