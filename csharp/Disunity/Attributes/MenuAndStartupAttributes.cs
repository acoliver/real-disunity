// Attributes that label serialized fields, add asset menu entries and mark start-up methods, written from the engine's own
// knowledge of what each one holds and checked against the player's data.

namespace UnityEngine
{
    /// <summary>A heading shown above a serialized field.</summary>
    [AttributeUsage(AttributeTargets.Field, Inherited = true, AllowMultiple = true)]
    public class HeaderAttribute(string header) : PropertyAttribute
    {
        public readonly string header = header;
    }

    /// <summary>Limits a serialized number to the range from <c>min</c> to <c>max</c> in an editor; the engine keeps the
    /// bounds and does not clamp.</summary>
    [AttributeUsage(AttributeTargets.Field, Inherited = true, AllowMultiple = false)]
    public sealed class RangeAttribute(float min, float max) : PropertyAttribute
    {
        public readonly float min = min;
        public readonly float max = max;
    }

    /// <summary>Adds an entry for creating assets of a scriptable object type to an editor's menu: its menu path, the file
    /// name a new asset gets and its place in the menu.</summary>
    [AttributeUsage(AttributeTargets.Class, Inherited = false, AllowMultiple = false)]
    public sealed class CreateAssetMenuAttribute : Attribute
    {
        public string menuName { get; set; } = "";
        public string fileName { get; set; } = "";
        public int order { get; set; }
    }

    /// <summary>When a start-up method runs. The values are the ones a player's RuntimeInitializeOnLoads.json stores; the
    /// phases run in the order 4, 2, 3, 1, 0.</summary>
    public enum RuntimeInitializeLoadType
    {
        AfterSceneLoad = 0,
        BeforeSceneLoad = 1,
        AfterAssembliesLoaded = 2,
        BeforeSplashScreen = 3,
        SubsystemRegistration = 4,
    }

    /// <summary>Marks a static method that runs during player start, in the phase <c>loadType</c> names; after the first
    /// scene loads unless it says otherwise.</summary>
    [AttributeUsage(AttributeTargets.Method, AllowMultiple = false)]
    public class RuntimeInitializeOnLoadMethodAttribute : Scripting.PreserveAttribute
    {
        public RuntimeInitializeOnLoadMethodAttribute() => loadType = RuntimeInitializeLoadType.AfterSceneLoad;

        public RuntimeInitializeOnLoadMethodAttribute(RuntimeInitializeLoadType loadType) => this.loadType = loadType;

        public RuntimeInitializeLoadType loadType { get; set; }
    }
}
