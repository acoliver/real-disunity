namespace UnityEngine;

/// <summary>
/// The modules of a <see cref="ParticleSystem"/>. Each is a view onto the system's settings, so a copy of a module
/// struct reads and writes the same values. Defaults are the engine's choice. The engine does not simulate, so the
/// values are stored and read back and nothing acts on them.
/// </summary>
public sealed partial class ParticleSystem
{
    public struct MainModule
    {
        private readonly ModuleStore _m;
        internal MainModule(ParticleSettings settings) => _m = new(settings, "main");

        /// <summary>Seconds of one cycle, 5 at first.</summary>
        public float duration { get => _m.Get(5f); set => _m.Set(value); }
        public bool loop { get => _m.Get(true); set => _m.Set(value); }
        public bool prewarm { get => _m.Get(false); set => _m.Set(value); }
        public MinMaxCurve startDelay { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }
        public MinMaxCurve startLifetime { get => _m.Get<MinMaxCurve>(5f); set => _m.Set(value); }
        public MinMaxCurve startSpeed { get => _m.Get<MinMaxCurve>(5f); set => _m.Set(value); }
        public MinMaxCurve startSize { get => _m.Get<MinMaxCurve>(1f); set => _m.Set(value); }
        public MinMaxCurve startRotation { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }
        public MinMaxGradient startColor { get => _m.Get<MinMaxGradient>(Color.white); set => _m.Set(value); }
        public MinMaxCurve gravityModifier { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }
        public ParticleSystemSimulationSpace simulationSpace { get => _m.Get(ParticleSystemSimulationSpace.Local); set => _m.Set(value); }
        public float simulationSpeed { get => _m.Get(1f); set => _m.Set(value); }
        public ParticleSystemScalingMode scalingMode { get => _m.Get(ParticleSystemScalingMode.Local); set => _m.Set(value); }
        public bool playOnAwake { get => _m.Get(true); set => _m.Set(value); }
        public bool useUnscaledTime { get => _m.Get(false); set => _m.Set(value); }
        public Transform? customSimulationSpace { get => _m.Get<Transform?>(null); set => _m.Set(value); }

        /// <summary>The most particles the system holds, 1000 at first; a negative value is stored as 0.</summary>
        public int maxParticles { get => _m.Get(1000); set => _m.Set(Math.Max(0, value)); }
    }

    public struct EmissionModule
    {
        private readonly ModuleStore _m;
        internal EmissionModule(ParticleSettings settings) => _m = new(settings, "emission");

        public bool enabled { get => _m.Get(true); set => _m.Set(value); }
        public MinMaxCurve rateOverTime { get => _m.Get<MinMaxCurve>(10f); set => _m.Set(value); }
        public MinMaxCurve rateOverDistance { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }
    }

    public struct ShapeModule
    {
        private readonly ModuleStore _m;
        internal ShapeModule(ParticleSettings settings) => _m = new(settings, "shape");

        public bool enabled { get => _m.Get(true); set => _m.Set(value); }
        public ParticleSystemShapeType shapeType { get => _m.Get(ParticleSystemShapeType.Cone); set => _m.Set(value); }
        public float angle { get => _m.Get(25f); set => _m.Set(value); }
        public float radius { get => _m.Get(1f); set => _m.Set(value); }
        public float arc { get => _m.Get(360f); set => _m.Set(value); }
        public Vector3 position { get => _m.Get(Vector3.zero); set => _m.Set(value); }
        public Vector3 rotation { get => _m.Get(Vector3.zero); set => _m.Set(value); }
        public Vector3 scale { get => _m.Get(Vector3.one); set => _m.Set(value); }
    }

    public struct VelocityOverLifetimeModule
    {
        private readonly ModuleStore _m;
        internal VelocityOverLifetimeModule(ParticleSettings settings) => _m = new(settings, "velocityOverLifetime");

        public bool enabled { get => _m.Get(false); set => _m.Set(value); }
        public MinMaxCurve x { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }
        public MinMaxCurve y { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }
        public MinMaxCurve z { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }
        public ParticleSystemSimulationSpace space { get => _m.Get(ParticleSystemSimulationSpace.Local); set => _m.Set(value); }
    }

    public struct ColorOverLifetimeModule
    {
        private readonly ModuleStore _m;
        internal ColorOverLifetimeModule(ParticleSettings settings) => _m = new(settings, "colorOverLifetime");

        public bool enabled { get => _m.Get(false); set => _m.Set(value); }
        public MinMaxGradient color { get => _m.Get<MinMaxGradient>(Color.white); set => _m.Set(value); }
    }

    public struct SizeOverLifetimeModule
    {
        private readonly ModuleStore _m;
        internal SizeOverLifetimeModule(ParticleSettings settings) => _m = new(settings, "sizeOverLifetime");

        public bool enabled { get => _m.Get(false); set => _m.Set(value); }
        public MinMaxCurve size { get => _m.Get<MinMaxCurve>(1f); set => _m.Set(value); }
        public bool separateAxes { get => _m.Get(false); set => _m.Set(value); }
    }

    public struct NoiseModule
    {
        private readonly ModuleStore _m;
        internal NoiseModule(ParticleSettings settings) => _m = new(settings, "noise");

        public bool enabled { get => _m.Get(false); set => _m.Set(value); }
        public MinMaxCurve strength { get => _m.Get<MinMaxCurve>(1f); set => _m.Set(value); }
        public float frequency { get => _m.Get(0.5f); set => _m.Set(value); }
        public MinMaxCurve scrollSpeed { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }
    }

    public struct TrailModule
    {
        private readonly ModuleStore _m;
        internal TrailModule(ParticleSettings settings) => _m = new(settings, "trails");

        public bool enabled { get => _m.Get(false); set => _m.Set(value); }
        public float ratio { get => _m.Get(1f); set => _m.Set(value); }
        public MinMaxCurve lifetime { get => _m.Get<MinMaxCurve>(1f); set => _m.Set(value); }
        public float minVertexDistance { get => _m.Get(0.2f); set => _m.Set(value); }
        public bool worldSpace { get => _m.Get(false); set => _m.Set(value); }
        public bool dieWithParticles { get => _m.Get(true); set => _m.Set(value); }
    }

    public struct CustomDataModule
    {
        private readonly ModuleStore _m;
        internal CustomDataModule(ParticleSettings settings) => _m = new(settings, "customData");

        public bool enabled { get => _m.Get(false); set => _m.Set(value); }
    }

    public struct TextureSheetAnimationModule
    {
        private readonly ModuleStore _m;
        internal TextureSheetAnimationModule(ParticleSettings settings) => _m = new(settings, "textureSheetAnimation");

        public bool enabled { get => _m.Get(false); set => _m.Set(value); }
        public ParticleSystemAnimationMode mode { get => _m.Get(ParticleSystemAnimationMode.Grid); set => _m.Set(value); }
        public ParticleSystemAnimationTimeMode timeMode { get => _m.Get(ParticleSystemAnimationTimeMode.Lifetime); set => _m.Set(value); }
        public int numTilesX { get => _m.Get(1); set => _m.Set(Math.Max(1, value)); }
        public int numTilesY { get => _m.Get(1); set => _m.Set(Math.Max(1, value)); }
        public ParticleSystemAnimationType animation { get => _m.Get(ParticleSystemAnimationType.WholeSheet); set => _m.Set(value); }
        public int rowIndex { get => _m.Get(0); set => _m.Set(value); }
        public float cycleCount { get => _m.Get(1f); set => _m.Set(value); }
        public float fps { get => _m.Get(30f); set => _m.Set(value); }
        public MinMaxCurve startFrame { get => _m.Get<MinMaxCurve>(0f); set => _m.Set(value); }

        /// <summary>The frame along a particle's life: a linear curve from 0 to 1 at first.</summary>
        public MinMaxCurve frameOverTime
        {
            get => _m.Get(new MinMaxCurve(1f, AnimationCurve.Linear(0f, 0f, 1f, 1f)));
            set => _m.Set(value);
        }
    }
}
