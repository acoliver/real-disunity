using Disunity.Hosting;
using UnityEngine.Rendering;

namespace UnityEngine;

public enum BuiltinSpriteMaterial : uint { Unlit = 0, Lit = 1 }

/// <summary>
/// A shader and the values set for it, kept in managed memory; nothing draws with it yet except the two
/// built-in sprite materials, which the native 2D pipeline draws; a sprite renderer with any other material draws
/// as the unlit sprite material. A material made from a <see cref="Shader"/>
/// or another material has no shader declarations to consult, so it knows only what was set on it: see
/// <see cref="Material.HasProperty(string)"/>. Unity's convenience members map to fixed names, as it does for
/// a shader without [MainColor] and [MainTexture]: <see cref="color"/> is _Color and <see cref="mainTexture"/>,
/// <see cref="mainTextureOffset"/> and <see cref="mainTextureScale"/> are _MainTex. The sprite materials start
/// with _Color white, as the sprite shaders declare; for any other shader a property never set reads as zero,
/// so <see cref="color"/> reads (0, 0, 0, 0) until it is set. The sprite materials are built in and cannot be
/// destroyed. <c>new Material(Material.SpriteLit)</c> is a copy that keeps the native sprite identity but is not
/// the one the native 2D pipeline draws with, so editing the copy does not change what a renderer draws.
/// </summary>
public partial class Material : Object
{
    // Both sprite shaders queue as Transparent (3000). Sprites/Default is one subshader of one pass; the URP
    // lit sprite shader's pass count is not known here.
    private static readonly Shader SpriteUnlitShader = DisunityHost.RegisterShader("Sprites/Default").Declare(3000, 1, 1);
    private static readonly Shader SpriteLitShader =
        DisunityHost.RegisterShader("Universal Render Pipeline/2D/Sprite-Lit-Default").Declare(3000, null, null);
    private static readonly int ColorId = Shader.PropertyToID("_Color");
    private static readonly int MainTexId = Shader.PropertyToID("_MainTex");

    public static Material SpriteUnlit { get; } = new(BuiltinSpriteMaterial.Unlit);
    public static Material SpriteLit { get; } = new(BuiltinSpriteMaterial.Lit);

    private readonly PropertyStore _properties = new();
    private readonly List<string> _keywords = new();
    private readonly Dictionary<string, string> _overrideTags = new();
    private readonly Dictionary<string, bool> _passEnabled = new();
    private readonly BuiltinSpriteMaterial? _sprite;
    private int _renderQueue = -1;

    internal override string? DestroyRefusal =>
        ReferenceEquals(this, SpriteUnlit) || ReferenceEquals(this, SpriteLit)
            ? $"The built-in sprite material '{name}' (Material.SpriteUnlit and Material.SpriteLit) cannot be destroyed: renderers draw with it"
            : null;

    /// <summary>The built-in sprite material this is; only the two sprite materials have one.</summary>
    public BuiltinSpriteMaterial SpriteMaterial =>
        _sprite ?? throw new NotSupportedException("Material.SpriteMaterial is not supported yet for a material other than the built-in sprite materials");

    /// <summary>The built-in sprite material the native 2D pipeline draws this as: itself for a sprite material or a copy
    /// of one, and the unlit sprite material for any other material.</summary>
    internal BuiltinSpriteMaterial NativeSpriteIdentity => _sprite ?? BuiltinSpriteMaterial.Unlit;

    public Material(BuiltinSpriteMaterial material)
    {
        if (material is not (BuiltinSpriteMaterial.Unlit or BuiltinSpriteMaterial.Lit))
            throw new NotSupportedException($"unsupported sprite material {material}");
        _sprite = material;
        shader = material == BuiltinSpriteMaterial.Unlit ? SpriteUnlitShader : SpriteLitShader;
        color = Color.white;
        name = material == BuiltinSpriteMaterial.Unlit ? "Sprites-Default" : "Sprite-Lit-Default";
    }

    public Material(Shader shader)
    {
        ArgumentNullException.ThrowIfNull(shader);
        this.shader = shader;
    }

    public Material(Material source)
    {
        ArgumentNullException.ThrowIfNull(source);
        _sprite = source._sprite;
        CopyStateFrom(source);
        name = source.name;
    }

    [Obsolete("Creating materials from shader source code is no longer supported. Use Shader assets instead.")]
    public Material(string contents) => throw Unsupported("Material(string)");

    [Obsolete("Creating materials from shader source code is no longer supported. Use Shader assets instead.")]
    public static Material Create(string scriptContents) => throw Unsupported(nameof(Create));

    internal static Material FromNative(uint value) => value switch
    {
        0 => SpriteUnlit, 1 => SpriteLit,
        _ => throw new InvalidOperationException($"invalid native sprite material {value}")
    };

    private void CopyStateFrom(Material source)
    {
        shader = source.shader;
        _properties.CopyFrom(source._properties);
        _keywords.Clear();
        _keywords.AddRange(source._keywords);
        _renderQueue = source._renderQueue;
        globalIlluminationFlags = source.globalIlluminationFlags;
        enableInstancing = source.enableInstancing;
        doubleSidedGI = source.doubleSidedGI;
        _overrideTags.Clear();
        foreach (var (tag, value) in source._overrideTags) _overrideTags[tag] = value;
        _passEnabled.Clear();
        foreach (var (pass, enabled) in source._passEnabled) _passEnabled[pass] = enabled;
    }

    public Shader shader { get; set; } = null!;

    public Color color { get => GetColor(ColorId); set => SetColor(ColorId, value); }
    public Texture? mainTexture { get => GetTexture(MainTexId); set => SetTexture(MainTexId, value!); }
    public Vector2 mainTextureOffset { get => GetTextureOffset(MainTexId); set => SetTextureOffset(MainTexId, value); }
    public Vector2 mainTextureScale { get => GetTextureScale(MainTexId); set => SetTextureScale(MainTexId, value); }

    /// <summary>The queue the material renders in; -1 set on the material means the shader's queue.</summary>
    public int renderQueue
    {
        get => _renderQueue >= 0 ? _renderQueue : shader.renderQueue;
        set => _renderQueue = value;
    }

    /// <summary>The queue stored on the material, or -1 when it comes from the shader.</summary>
    public int rawRenderQueue => _renderQueue;

    public MaterialGlobalIlluminationFlags globalIlluminationFlags { get; set; }
    public bool doubleSidedGI { get; set; }
    public bool enableInstancing { get; set; }
    public int passCount => shader.passCount;

    public string[] shaderKeywords
    {
        get => _keywords.ToArray();
        set
        {
            ArgumentNullException.ThrowIfNull(value);
            _keywords.Clear();
            foreach (var keyword in value) EnableKeyword(keyword);
        }
    }

    public LocalKeyword[] enabledKeywords
    {
        get => _keywords.Select(keyword => new LocalKeyword(shader, keyword)).ToArray();
        set
        {
            ArgumentNullException.ThrowIfNull(value);
            _keywords.Clear();
            foreach (var keyword in value) EnableKeyword(keyword.name);
        }
    }

    public void EnableKeyword(string keyword) => SetKeyword(keyword, true);

    public void DisableKeyword(string keyword) => SetKeyword(keyword, false);

    public bool IsKeywordEnabled(string keyword)
    {
        ArgumentNullException.ThrowIfNull(keyword);
        return _keywords.Contains(keyword);
    }

    public void EnableKeyword(ref LocalKeyword keyword) => SetKeyword(keyword.name, true);

    public void DisableKeyword(ref LocalKeyword keyword) => SetKeyword(keyword.name, false);

    public void SetKeyword(ref LocalKeyword keyword, bool value) => SetKeyword(keyword.name, value);

    public bool IsKeywordEnabled(ref LocalKeyword keyword) => IsKeywordEnabled(keyword.name);

    private void SetKeyword(string keyword, bool enabled)
    {
        ArgumentNullException.ThrowIfNull(keyword);
        if (enabled && !_keywords.Contains(keyword)) _keywords.Add(keyword);
        else if (!enabled) _keywords.Remove(keyword);
    }

    public void SetShaderPassEnabled(string passName, bool enabled)
    {
        ArgumentNullException.ThrowIfNull(passName);
        _passEnabled[passName] = enabled;
    }

    /// <summary>A pass is enabled until SetShaderPassEnabled turns it off, including a pass the shader does not have.</summary>
    public bool GetShaderPassEnabled(string passName)
    {
        ArgumentNullException.ThrowIfNull(passName);
        return _passEnabled.GetValueOrDefault(passName, true);
    }

    public string GetPassName(int pass) => throw Unsupported(nameof(GetPassName));

    public int FindPass(string passName) => throw Unsupported(nameof(FindPass));

    /// <summary>An empty value removes the override.</summary>
    public void SetOverrideTag(string tag, string val)
    {
        ArgumentNullException.ThrowIfNull(tag);
        ArgumentNullException.ThrowIfNull(val);
        if (val.Length == 0) _overrideTags.Remove(tag);
        else _overrideTags[tag] = val;
    }

    /// <summary>The override set on this material, or <paramref name="defaultValue"/>. The shader's own tags are
    /// not available, so a tag the shader declares but this material does not override also reads as
    /// <paramref name="defaultValue"/>, and <paramref name="searchFallbacks"/> has no effect.</summary>
    public string GetTag(string tag, bool searchFallbacks, string defaultValue)
    {
        ArgumentNullException.ThrowIfNull(tag);
        return _overrideTags.GetValueOrDefault(tag, defaultValue);
    }

    public string GetTag(string tag, bool searchFallbacks) => GetTag(tag, searchFallbacks, "");

    /// <summary>Binds the pass for drawing, which needs a GPU.</summary>
    public bool SetPass(int pass) => throw Unsupported(nameof(SetPass));

    /// <summary>Copies the shader, every property, the keywords, the render queue and the other settings from <paramref name="mat"/>.</summary>
    public void CopyPropertiesFromMaterial(Material mat)
    {
        ArgumentNullException.ThrowIfNull(mat);
        CopyStateFrom(mat);
    }

    /// <summary>Choosing the properties both shaders declare needs the shaders' declarations.</summary>
    public void CopyMatchingPropertiesFromMaterial(Material mat) => throw Unsupported(nameof(CopyMatchingPropertiesFromMaterial));

    /// <summary>
    /// Sets each float and color or vector property that both materials have to the interpolation of
    /// their values, with <paramref name="t"/> kept between 0 and 1. Properties only one has are left alone.
    /// </summary>
    public void Lerp(Material start, Material end, float t)
    {
        ArgumentNullException.ThrowIfNull(start);
        ArgumentNullException.ThrowIfNull(end);
        t = Mathf.Clamp01(t);
        foreach (var (id, from) in start._properties.Floats)
            if (end._properties.Floats.TryGetValue(id, out var to)) _properties.Floats[id] = Mathf.Lerp(from, to, t);
        foreach (var (id, from) in start._properties.Vectors)
            if (end._properties.Vectors.TryGetValue(id, out var to)) _properties.Vectors[id] = Vector4.Lerp(from, to, t);
    }

    public void SetTextureOffset(string name, Vector2 value) => SetTextureOffset(Shader.PropertyToID(name), value);

    public void SetTextureOffset(int nameID, Vector2 value) => _properties.Offsets[nameID] = value;

    public void SetTextureScale(string name, Vector2 value) => SetTextureScale(Shader.PropertyToID(name), value);

    public void SetTextureScale(int nameID, Vector2 value) => _properties.Scales[nameID] = value;

    public Vector2 GetTextureOffset(string name) => GetTextureOffset(Shader.PropertyToID(name));

    public Vector2 GetTextureOffset(int nameID) => _properties.Offsets.GetValueOrDefault(nameID, Vector2.zero);

    public Vector2 GetTextureScale(string name) => GetTextureScale(Shader.PropertyToID(name));

    public Vector2 GetTextureScale(int nameID) => _properties.Scales.GetValueOrDefault(nameID, Vector2.one);

    public GraphicsBufferHandle GetBuffer(string name) => throw Unsupported(nameof(GetBuffer));

    public GraphicsBufferHandle GetConstantBuffer(string name) => throw Unsupported(nameof(GetConstantBuffer));

    /// <summary>The texture properties set on this material; the shader's declared texture slots are not known.</summary>
    public string[] GetTexturePropertyNames() => Names(_properties.Textures.Keys);

    public int[] GetTexturePropertyNameIDs() => _properties.Textures.Keys.ToArray();

    public void GetTexturePropertyNames(List<string> outNames)
    {
        ArgumentNullException.ThrowIfNull(outNames);
        outNames.Clear();
        outNames.AddRange(GetTexturePropertyNames());
    }

    public void GetTexturePropertyNameIDs(List<int> outNames)
    {
        ArgumentNullException.ThrowIfNull(outNames);
        outNames.Clear();
        outNames.AddRange(GetTexturePropertyNameIDs());
    }

    /// <summary>The names of the properties of a type that were set on this material.</summary>
    public string[] GetPropertyNames(MaterialPropertyType type) => type switch
    {
        MaterialPropertyType.Float => Names(_properties.Floats.Keys),
        MaterialPropertyType.Int => Names(_properties.Ints.Keys),
        MaterialPropertyType.Vector => Names(_properties.Vectors.Keys),
        MaterialPropertyType.Matrix => Names(_properties.Matrices.Keys),
        MaterialPropertyType.Texture => Names(_properties.Textures.Keys),
        MaterialPropertyType.ConstantBuffer => Names(_properties.ConstantBuffers.Keys),
        MaterialPropertyType.ComputeBuffer => Names(_properties.Buffers.Keys),
        _ => throw new ArgumentOutOfRangeException(nameof(type)),
    };

    private static string[] Names(IEnumerable<int> ids) => ids.Select(id => Shader.PropertyName(id) ?? "").ToArray();

    internal override Object CloneAsset() => Named(new Material(this));

    private static NotSupportedException Unsupported(string member) =>
        new($"Material.{member} is not supported yet: the engine has no shader declarations or GPU");
}
