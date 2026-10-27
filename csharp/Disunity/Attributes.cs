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

/// <summary>When a <see cref="RuntimeInitializeOnLoadMethodAttribute"/> method runs during startup.</summary>
public enum RuntimeInitializeLoadType
{
    AfterSceneLoad = 0,
    BeforeSceneLoad = 1,
    AfterAssembliesLoaded = 2,
    BeforeSplashScreen = 3,
    SubsystemRegistration = 4,
}

/// <summary>
/// Marks a static method to run once during startup. A built player lists these methods in
/// <c>RuntimeInitializeOnLoads.json</c>, which is what the host reads (see
/// <c>Disunity.Hosting.RuntimeHooks</c>).
/// </summary>
[AttributeUsage(AttributeTargets.Method)]
public class RuntimeInitializeOnLoadMethodAttribute : Attribute
{
    public RuntimeInitializeOnLoadMethodAttribute() : this(RuntimeInitializeLoadType.AfterSceneLoad) { }

    public RuntimeInitializeOnLoadMethodAttribute(RuntimeInitializeLoadType loadType) => this.loadType = loadType;

    public RuntimeInitializeLoadType loadType { get; set; }
}
