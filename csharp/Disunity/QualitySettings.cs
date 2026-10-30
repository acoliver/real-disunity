using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The quality settings, held as static state that resets to the defaults below when a world loads.
/// The defaults are the engine's choices: one level named "Default", anti-aliasing off, no mipmap
/// limit, no LOD level limit, two queued frames, vertical sync every frame, LOD bias 1, shadow
/// distance 150, all shadows, gamma color space. The engine does not know the values of each of a
/// project's quality levels yet, so switching levels changes only the current index and leaves
/// every stored value as it is. <see cref="SetQualityLevel(int)"/> clamps an index past the last
/// level to the last level. Each level can name a render pipeline asset; a host supplies them with
/// <c>SetPipelines</c> and <see cref="renderPipeline"/> reads and replaces the current level's.
/// </summary>
public static class QualitySettings
{
    private static readonly object Gate = new();
    private static string[] _names = ["Default"];
    private static RenderPipelineAsset?[] _pipelines = [null];
    private static int _level;
    private static int _antiAliasing;
    private static int _mipmapLimit;
    private static int _maximumLod;
    private static int _queuedFrames = 2;
    private static int _vSync = 1;
    private static float _lodBias = 1f;
    private static float _shadowDistance = 150f;
    private static ShadowQuality _shadows = ShadowQuality.All;
    private static ColorSpace _colorSpace = ColorSpace.Gamma;

    /// <summary>The multisample count: 0, 2, 4 or 8.</summary>
    public static int antiAliasing
    {
        get { lock (Gate) return _antiAliasing; }
        set
        {
            if (value is not (0 or 2 or 4 or 8)) throw new ArgumentOutOfRangeException(nameof(value), value, "antiAliasing is 0, 2, 4 or 8");
            lock (Gate) _antiAliasing = value;
        }
    }

    public static int globalTextureMipmapLimit
    {
        get { lock (Gate) return _mipmapLimit; }
        set { lock (Gate) _mipmapLimit = NotNegative(value); }
    }

    public static int maximumLODLevel
    {
        get { lock (Gate) return _maximumLod; }
        set { lock (Gate) _maximumLod = NotNegative(value); }
    }

    /// <summary>Frames the CPU may queue ahead of the GPU: 1 to 10.</summary>
    public static int maxQueuedFrames
    {
        get { lock (Gate) return _queuedFrames; }
        set { lock (Gate) _queuedFrames = InRange(value, 1, 10); }
    }

    /// <summary>Vertical blanks to wait between frames: 0 to 4.</summary>
    public static int vSyncCount
    {
        get { lock (Gate) return _vSync; }
        set { lock (Gate) _vSync = InRange(value, 0, 4); }
    }

    /// <summary>The LOD distance multiplier; finite and greater than 0.</summary>
    public static float lodBias
    {
        get { lock (Gate) return _lodBias; }
        set
        {
            if (!float.IsFinite(value) || value <= 0) throw new ArgumentOutOfRangeException(nameof(value), value, "lodBias is finite and positive");
            lock (Gate) _lodBias = value;
        }
    }

    public static float shadowDistance
    {
        get { lock (Gate) return _shadowDistance; }
        set
        {
            if (float.IsNaN(value) || value < 0) throw new ArgumentOutOfRangeException(nameof(value), value, "shadowDistance is not negative");
            lock (Gate) _shadowDistance = value;
        }
    }

    public static ShadowQuality shadows
    {
        get { lock (Gate) return _shadows; }
        set
        {
            if (!Enum.IsDefined(value)) throw new ArgumentOutOfRangeException(nameof(value), value, "unknown shadow quality");
            lock (Gate) _shadows = value;
        }
    }

    /// <summary>The color space the host reports; gamma until it says otherwise.</summary>
    public static ColorSpace activeColorSpace
    {
        get { lock (Gate) return _colorSpace; }
    }

    /// <summary>The render pipeline asset of the current level, or null when the level names none.
    /// Setting it replaces the current level's asset.</summary>
    public static RenderPipelineAsset? renderPipeline
    {
        get { lock (Gate) return _pipelines[_level]; }
        set { lock (Gate) _pipelines[_level] = value; }
    }

    /// <summary>The render pipeline asset of level <paramref name="index"/>, or null when it names none.</summary>
    public static RenderPipelineAsset? GetRenderPipelineAssetAt(int index)
    {
        lock (Gate)
        {
            if (index < 0 || index >= _pipelines.Length) throw new ArgumentOutOfRangeException(nameof(index), index, $"the level is 0 to {_pipelines.Length - 1}");
            return _pipelines[index];
        }
    }

    /// <summary>The assets of every level that names one, each once.</summary>
    internal static RenderPipelineAsset[] ConfiguredPipelines()
    {
        lock (Gate) return _pipelines.Where(asset => asset).Distinct().ToArray()!;
    }

    /// <summary>Gives each level its render pipeline asset, in level order.</summary>
    internal static void SetPipelines(IReadOnlyList<RenderPipelineAsset?> assets)
    {
        ArgumentNullException.ThrowIfNull(assets);
        lock (Gate)
        {
            if (assets.Count != _names.Length) throw new ArgumentException($"{assets.Count} render pipeline assets for {_names.Length} quality levels", nameof(assets));
            _pipelines = assets.ToArray();
        }
    }

    /// <summary>A copy of the quality level names, in level order.</summary>
    public static string[] names
    {
        get { lock (Gate) return (string[])_names.Clone(); }
    }

    public static int GetQualityLevel()
    {
        lock (Gate) return _level;
    }

    /// <summary>
    /// Selects a quality level. An index at or above the level count selects the last level: the
    /// original Old World player calls this with 3 while it defines three levels and runs on, so
    /// the engine clamps. A negative index throws, which is the engine's choice because nothing
    /// observed passes one.
    /// </summary>
    public static void SetQualityLevel(int index) => SetQualityLevel(index, true);

    /// <inheritdoc cref="SetQualityLevel(int)"/>
    public static void SetQualityLevel(int index, bool applyExpensiveChanges)
    {
        if (index < 0) throw new ArgumentOutOfRangeException(nameof(index), index, "a quality level index is not negative");
        lock (Gate) _level = Math.Min(index, _names.Length - 1);
    }

    internal static void SetLevels(IReadOnlyList<string> levelNames, int current)
    {
        ArgumentNullException.ThrowIfNull(levelNames);
        if (levelNames.Count == 0) throw new ArgumentException("there must be at least one quality level", nameof(levelNames));
        if (current < 0 || current >= levelNames.Count) throw new ArgumentOutOfRangeException(nameof(current), current, "the current level is not one of the names");
        var copy = levelNames.ToArray();
        lock (Gate) (_names, _level, _pipelines) = (copy, current, new RenderPipelineAsset?[copy.Length]);
    }

    internal static void SetColorSpace(ColorSpace space)
    {
        if (space == ColorSpace.Uninitialized || !Enum.IsDefined(space)) throw new ArgumentOutOfRangeException(nameof(space), space, "the color space is Gamma or Linear");
        lock (Gate) _colorSpace = space;
    }

    internal static void Reset()
    {
        lock (Gate)
        {
            (_names, _pipelines) = (["Default"], [null]);
            (_level, _antiAliasing, _mipmapLimit, _maximumLod, _queuedFrames, _vSync) = (0, 0, 0, 0, 2, 1);
            (_lodBias, _shadowDistance, _shadows, _colorSpace) = (1f, 150f, ShadowQuality.All, ColorSpace.Gamma);
        }
    }

    private static int NotNegative(int value) =>
        value >= 0 ? value : throw new ArgumentOutOfRangeException(nameof(value), value, "the value is not negative");

    private static int InRange(int value, int low, int high) =>
        value >= low && value <= high ? value : throw new ArgumentOutOfRangeException(nameof(value), value, $"the value is {low} to {high}");
}
