using UnityEngine.Rendering;

namespace UnityEngine
{
    /// <summary>How fog thickens with distance.</summary>
    public enum FogMode
    {
        Linear = 1,
        Exponential = 2,
        ExponentialSquared = 3,
    }

    /// <summary>
    /// The scene's environment values: fog, ambient light, reflections, flares and the skybox. A new world starts from the
    /// values of a new scene and a world reload restores them; a script's writes are stored and read back. The ambient light
    /// (mode, colours, intensity and probe) and the default reflection (mode, intensity and cube) are the world's one
    /// environment record, which a scene load fills from the scene's render settings and the host draws from (see
    /// <c>RenderSettingsEnvironment.cs</c>); the other values are managed, and nothing applies them to a frame: a render
    /// pipeline reads them and sends what it needs as shader globals. <see cref="ambientIntensity"/> stops at 0 and 8,
    /// <see cref="reflectionIntensity"/> at 0 and 1 and <see cref="reflectionBounces"/> at 1 and 5 (the engine's choice).
    /// <see cref="sun"/> is null until a script sets it: the engine does not pick the brightest directional light.
    /// </summary>
    public static partial class RenderSettings
    {
        private static readonly object Gate = new();
        private static State _state = new();

        private sealed class State
        {
            public bool Fog;
            public FogMode FogMode = FogMode.ExponentialSquared;
            public Color FogColor = new(0.5f, 0.5f, 0.5f, 1f);
            public float FogDensity = 0.01f, FogStart, FogEnd = 300f;
            public Color Subtractive = new(0.42f, 0.478f, 0.627f, 1f);
            public int ReflectionBounces = 1, ReflectionResolution = 128;
            public float Halo = 0.5f, FlareFade = 3f, FlareStrength = 1f;
            public Material? Skybox;
            public Light? Sun;
            // The textures the world's environment record names, kept so the record's identities read back as objects and
            // so the textures stay alive while the record names them.
            public Texture? CustomReflection, SkyboxReflection;
        }

        /// <summary>Restores the values of a new scene; the host calls it when a world unloads.</summary>
        internal static void Reset()
        {
            lock (Gate) _state = new State();
        }

        private static T Read<T>(Func<State, T> read)
        {
            lock (Gate) return read(_state);
        }

        private static void Write(Action<State> write)
        {
            lock (Gate) write(_state);
        }

        public static bool fog { get => Read(s => s.Fog); set => Write(s => s.Fog = value); }
        public static FogMode fogMode { get => Read(s => s.FogMode); set => Write(s => s.FogMode = value); }
        public static Color fogColor { get => Read(s => s.FogColor); set => Write(s => s.FogColor = value); }
        public static float fogDensity { get => Read(s => s.FogDensity); set => Write(s => s.FogDensity = value); }
        public static float fogStartDistance { get => Read(s => s.FogStart); set => Write(s => s.FogStart = value); }
        public static float fogEndDistance { get => Read(s => s.FogEnd); set => Write(s => s.FogEnd = value); }

        public static Color subtractiveShadowColor { get => Read(s => s.Subtractive); set => Write(s => s.Subtractive = value); }

        public static int defaultReflectionResolution { get => Read(s => s.ReflectionResolution); set => Write(s => s.ReflectionResolution = value); }
        public static int reflectionBounces { get => Read(s => s.ReflectionBounces); set => Write(s => s.ReflectionBounces = Math.Clamp(value, 1, 5)); }

        public static float haloStrength { get => Read(s => s.Halo); set => Write(s => s.Halo = value); }
        public static float flareFadeSpeed { get => Read(s => s.FlareFade); set => Write(s => s.FlareFade = value); }
        public static float flareStrength { get => Read(s => s.FlareStrength); set => Write(s => s.FlareStrength = value); }

        public static Material? skybox { get => Read(s => s.Skybox); set => Write(s => s.Skybox = value); }
        public static Light? sun { get => Read(s => s.Sun); set => Write(s => s.Sun = value); }
    }
}

namespace UnityEngine.Rendering
{
    /// <summary>Where the ambient light of a scene comes from.</summary>
    public enum AmbientMode
    {
        Skybox = 0,
        Trilight = 1,
        Flat = 3,
        Custom = 4,
    }

    /// <summary>Where the default reflection comes from.</summary>
    public enum DefaultReflectionMode
    {
        Skybox = 0,
        Custom = 1,
    }
}
