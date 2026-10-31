using System.Runtime.CompilerServices;

namespace UnityEngine;

/// <summary>
/// The values a <see cref="ParticleSystem"/>'s modules and a <see cref="ParticleSystemRenderer"/> hold, keyed by
/// module and member name. A value that was never written reads as the default its member passes.
/// </summary>
internal sealed class ParticleSettings
{
    private readonly Dictionary<string, object?> _values = new();

    public T Get<T>(string key, T fallback) => _values.TryGetValue(key, out var value) ? (T)value! : fallback;

    public void Set<T>(string key, T value) => _values[key] = value;

    public void CopyFrom(ParticleSettings other)
    {
        _values.Clear();
        foreach (var (key, value) in other._values) _values[key] = value;
    }
}

/// <summary>One module's view of the settings: <c>Get</c> and <c>Set</c> use the calling property's name as the key.</summary>
internal readonly struct ModuleStore(ParticleSettings settings, string module)
{
    public T Get<T>(T fallback, [CallerMemberName] string member = "") => settings.Get($"{module}.{member}", fallback);

    public void Set<T>(T value, [CallerMemberName] string member = "") => settings.Set($"{module}.{member}", value);
}
