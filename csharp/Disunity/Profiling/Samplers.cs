using System.Collections.Concurrent;

namespace UnityEngine.Profiling;

/// <summary>
/// A named profiler sampler. The engine has no profiler capture, so every sampler is valid, <see cref="Get"/>
/// returns the same sampler for the same name, and its recorder reads zero.
/// </summary>
public class Sampler
{
    private static readonly ConcurrentDictionary<string, Sampler> Samplers = new(StringComparer.Ordinal);

    private Recorder? _recorder;

    internal Sampler(string name) => this.name = name;

    public string name { get; }
    public bool isValid => true;

    public static Sampler Get(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        return Samplers.GetOrAdd(name, static key => new Sampler(key));
    }

    /// <summary>Adds the names of all samplers made so far to the list and returns how many there are.</summary>
    public static int GetNames(List<string> names)
    {
        ArgumentNullException.ThrowIfNull(names);
        names.AddRange(Samplers.Keys);
        return Samplers.Count;
    }

    public Recorder GetRecorder() => _recorder ??= new Recorder();

    /// <summary>Makes a custom sampler, or returns the one already made for the name.</summary>
    internal static CustomSampler CustomSamplerFor(string name)
    {
        ArgumentNullException.ThrowIfNull(name);
        var sampler = Samplers.AddOrUpdate(name, static key => new CustomSampler(key), static (key, existing) => existing as CustomSampler ?? new CustomSampler(key));
        return (CustomSampler)sampler;
    }
}

/// <summary>A sampler a script begins and ends itself. Nothing is recorded.</summary>
public sealed class CustomSampler : Sampler
{
    internal CustomSampler(string name) : base(name) { }

    public static CustomSampler Create(string name, bool collectGpuData = false) => CustomSamplerFor(name);

    public void Begin() { }
    public void Begin(Object targetObject) { }
    public void End() { }
}

/// <summary>Reads what a sampler recorded. The engine records nothing, so every value is zero. <c>enabled</c> is stored.</summary>
public sealed class Recorder
{
    internal Recorder() { }

    public static Recorder Get(string samplerName) => Sampler.Get(samplerName).GetRecorder();

    public bool isValid => true;
    public bool enabled { get; set; } = true;
    public long elapsedNanoseconds => 0;
    public long gpuElapsedNanoseconds => 0;
    public int sampleBlockCount => 0;
    public int gpuSampleBlockCount => 0;

    public void FilterToCurrentThread() { }
    public void CollectFromAllThreads() { }
}
