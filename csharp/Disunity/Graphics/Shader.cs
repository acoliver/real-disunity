using Disunity.Hosting;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A shader known to the engine by name. A shader exists once <see cref="DisunityHost.RegisterShader"/> has
/// registered it, and <see cref="Find"/> returns only registered shaders. The engine has no shader programs
/// or declarations beyond what a player's plan carries (queue, pass names, sub-shader count, the first sub-shader's tags and
/// the fallback name), so the members that read a shader's other passes, keywords or properties, or its undeclared passes or tags, throw
/// <see cref="NotSupportedException"/>, and so do renderQueue, passCount, subshaderCount and an unset maximumLOD,
/// except for the two built-in sprite shaders, whose queue (3000) and, for Sprites/Default, pass and subshader
/// counts (1) are known. The global properties and keywords are stored and read back, and sent to the world.
/// The globals are main-thread state, as in Unity, and fail on other threads; the global keyword
/// lists are locked. A shader has a resource in the world once something needs one; the global values and
/// keywords are sent to the world whenever they are set, and again when a world is loaded.
/// </summary>
public sealed partial class Shader : Object
{
    private static readonly NameTable Properties = new(1000);
    private static readonly object KeywordLock = new();
    private static readonly List<string> KnownKeywords = new();
    private static readonly List<string> EnabledKeywords = new();

    private int? _renderQueue, _passCount, _subshaderCount, _maximumLod;
    private string[]? _passNames;
    private bool _engineDefined;
    private WorldGraphics.Link _world;
    private string? _sourceFile;
    private long _sourcePathId;

    internal Shader(string name) => this.name = name;

    /// <summary>The shader's resource in the loaded world, made on first use with the shader's name and, when the player's plan
    /// gave one, the file and object its programs are in. A shader made by name alone, such as a fallback, has no source.</summary>
    internal WorldGraphics.Link WorldLink
    {
        get
        {
            if (_world.IsCurrent) return _world;
            _world = WorldGraphics.CreateShader(name);
            if (_sourceFile is not null) WorldGraphics.WriteShaderSource(_world, _sourceFile, _sourcePathId);
            return _world;
        }
    }

    /// <summary>Records the serialized file and object the shader's programs are read from, sending it to the world when the
    /// shader already has a resource there.</summary>
    internal void SetSource(string file, long pathId)
    {
        (_sourceFile, _sourcePathId) = (file, pathId);
        if (_world.IsCurrent) WorldGraphics.WriteShaderSource(_world, file, pathId);
    }

    internal string? SourceFile => _sourceFile;
    internal long SourcePathId => _sourcePathId;

    /// <summary>Records a declaration. A plan's declaration replaces the engine's, with everything the engine declared dropped, and
    /// the engine's declaration leaves a plan's alone; otherwise it records the queue and what the declaration knows of the counts,
    /// the pass names, the tags and the fallback, keeping what an earlier declaration knew where this one leaves it out.</summary>
    internal Shader Declare(ShaderDeclaration declaration)
    {
        var engine = declaration.Origin == DeclarationOrigin.Engine;
        if (engine && _renderQueue is not null && !_engineDefined) return this;
        if (!engine && _engineDefined) ForgetDeclaration();
        _engineDefined = engine;
        _renderQueue = declaration.RenderQueue;
        _passCount = declaration.PassCount ?? _passCount;
        _subshaderCount = declaration.SubshaderCount ?? _subshaderCount;
        _passNames = declaration.PassNames?.ToArray() ?? _passNames;
        DeclareTags(declaration);
        return this;
    }

    private void ForgetDeclaration()
    {
        (_renderQueue, _passCount, _subshaderCount, _passNames) = (null, null, null, null);
        (_tags, _fallback) = (null, null);
    }

    /// <summary>Whether a declaration can stand beside what is declared: true when nothing is declared yet, when the engine
    /// defined what is declared (a plan replaces it) or the declaration is the engine's (it gives way), or else when the queue
    /// matches and each count, list of names, tag set and fallback that is known matches.</summary>
    internal bool Agrees(ShaderDeclaration declaration) =>
        _renderQueue is null || _engineDefined || declaration.Origin == DeclarationOrigin.Engine
        || (_renderQueue == declaration.RenderQueue && Same(_passCount, declaration.PassCount) && Same(_subshaderCount, declaration.SubshaderCount)
            && (_passNames is null || declaration.PassNames is null || _passNames.SequenceEqual(declaration.PassNames)) && TagsAgree(declaration));

    private static bool Same(int? known, int? other) => known is null || other is null || known == other;

    /// <summary>The names of the first sub-shader's passes in order. A shader whose names are not declared throws
    /// <see cref="NotSupportedException"/> naming <paramref name="member"/>.</summary>
    internal IReadOnlyList<string> PassNames(string member) =>
        _passNames ?? throw new NotSupportedException($"{member} is not supported yet: the engine has no pass names for shader '{name}'");

    private int Declared(int? value, string member) =>
        value ?? throw new NotSupportedException($"Shader.{member} is not supported yet: the engine has no declaration for shader '{name}'");

    public static ShaderHardwareTier globalShaderHardwareTier { get; set; } = ShaderHardwareTier.Tier3;
    public static int maximumChunksOverride { get; set; }
    public static int globalMaximumLOD { get; set; } = int.MaxValue;
    public static string globalRenderPipeline { get; set; } = "";

    /// <summary>The value set here; a shader's own LOD is not declared, so reading it before setting it throws.</summary>
    public int maximumLOD
    {
        get => Declared(_maximumLod, nameof(maximumLOD));
        set => _maximumLod = value;
    }

    public bool isSupported => !Destroyed;
    public int renderQueue => Declared(_renderQueue, nameof(renderQueue));
    public int passCount => Declared(_passCount, nameof(passCount));

    /// <summary>The declared pass count, or null when the engine has no declaration, for callers that validate a pass only when they can.</summary>
    internal int? DeclaredPassCount => _passCount;
    public int subshaderCount => Declared(_subshaderCount, nameof(subshaderCount));

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
        lock (KeywordLock)
            if (!KnownKeywords.Contains(name)) KnownKeywords.Add(name);
    }

    public static GlobalKeyword[] globalKeywords => Snapshot(KnownKeywords).Select(keyword => new GlobalKeyword(keyword)).ToArray();

    public static GlobalKeyword[] enabledGlobalKeywords => Snapshot(EnabledKeywords).Select(keyword => new GlobalKeyword(keyword)).ToArray();

    private static string[] Snapshot(List<string> keywords)
    {
        lock (KeywordLock) return keywords.ToArray();
    }

    public static void EnableKeyword(string keyword) => SetGlobalKeyword(keyword, true);

    public static void DisableKeyword(string keyword) => SetGlobalKeyword(keyword, false);

    public static bool IsKeywordEnabled(string keyword)
    {
        ArgumentNullException.ThrowIfNull(keyword);
        lock (KeywordLock) return EnabledKeywords.Contains(keyword);
    }

    public static void EnableKeyword(ref GlobalKeyword keyword) => SetGlobalKeyword(keyword.name, true);

    public static void DisableKeyword(ref GlobalKeyword keyword) => SetGlobalKeyword(keyword.name, false);

    public static void SetKeyword(ref GlobalKeyword keyword, bool value) => SetGlobalKeyword(keyword.name, value);

    public static bool IsKeywordEnabled(ref GlobalKeyword keyword) => IsKeywordEnabled(keyword.name);

    private static void SetGlobalKeyword(string keyword, bool enabled)
    {
        ArgumentNullException.ThrowIfNull(keyword);
        NoteGlobalKeyword(keyword);
        lock (KeywordLock)
        {
            if (enabled && !EnabledKeywords.Contains(keyword)) EnabledKeywords.Add(keyword);
            else if (!enabled) EnabledKeywords.Remove(keyword);
        }
        if (DisunityHost.IsLoaded) WorldGraphics.WriteKeyword(WorldGraphics.ScopeGlobal, WorldGraphics.GlobalTarget, keyword, enabled);
    }

    /// <summary>Sends the global keywords and properties to the world that was just loaded: they outlive worlds, the world's
    /// copy does not.</summary>
    internal static void SyncGlobalsToWorld()
    {
        var target = WorldGraphics.GlobalTarget;
        string[] enabled;
        lock (KeywordLock) enabled = EnabledKeywords.ToArray();
        foreach (var keyword in enabled) WorldGraphics.WriteKeyword(WorldGraphics.ScopeGlobal, target, keyword, true);
        WorldGraphics.SendAll(WorldGraphics.ScopeGlobal, target, GlobalStore);
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
