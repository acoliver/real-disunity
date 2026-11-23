using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine
{
    /// <summary>
    /// The ambient light and the default reflection of <see cref="RenderSettings"/>. They are not held here: the world holds one
    /// environment record (<c>DisunityEnvironment</c>), which these members read and write through the ABI, a scene load fills
    /// from the scene's render settings, and the host draws from. A section nobody wrote reads as the values of a new scene and
    /// stays unset in the record, so the host keeps its own defaults for it; the first write of any value of a section sets
    /// the whole section. The members need a loaded world.
    /// </summary>
    public static unsafe partial class RenderSettings
    {
        private delegate T Getter<out T>(DisunityEnvironment* record);

        private delegate void Setter(DisunityEnvironment* record);

        private static ulong World => DisunityHost.IsLoaded ? DisunityHost.WorldHandle : throw new InvalidOperationException("RenderSettings needs a loaded world");

        private static DisunityEnvironment ReadRecord()
        {
            DisunityEnvironment record;
            DisunityHost.ThrowIfError(NativeBindings.disunity_environment_read(World, &record));
            return record;
        }

        private static void WriteRecord(DisunityEnvironment* record) =>
            DisunityHost.ThrowIfError(NativeBindings.disunity_environment_write(World, record));

        private static T Get<T>(Getter<T> read)
        {
            lock (Gate)
            {
                var record = ReadRecord();
                return read(&record);
            }
        }

        private static void Edit(Setter edit, bool ambient)
        {
            lock (Gate)
            {
                var record = ReadRecord();
                edit(&record);
                if (ambient) record.ambient_set = 1;
                else record.reflection_set = 1;
                WriteRecord(&record);
            }
        }

        private static void SetAmbient(Setter edit) => Edit(edit, ambient: true);

        private static void SetReflection(Setter edit) => Edit(edit, ambient: false);

        private static Color ColorOf(float* values) => new(values[0], values[1], values[2], values[3]);

        private static void Put(float* target, Color color)
        {
            target[0] = Finite(color.r);
            target[1] = Finite(color.g);
            target[2] = Finite(color.b);
            target[3] = Finite(color.a);
        }

        /// <summary>The world holds finite values only, so a NaN or an infinity, which a game may set, is stored as 0.</summary>
        private static float Finite(float value) => float.IsFinite(value) ? value : 0f;

        public static AmbientMode ambientMode
        {
            get => Get(r => (AmbientMode)r->ambient_mode);
            set => SetAmbient(r => r->ambient_mode = (uint)value);
        }

        public static Color ambientSkyColor
        {
            get => Get(r => ColorOf(r->ambient_sky));
            set => SetAmbient(r => Put(r->ambient_sky, value));
        }

        public static Color ambientEquatorColor
        {
            get => Get(r => ColorOf(r->ambient_equator));
            set => SetAmbient(r => Put(r->ambient_equator, value));
        }

        public static Color ambientGroundColor
        {
            get => Get(r => ColorOf(r->ambient_ground));
            set => SetAmbient(r => Put(r->ambient_ground, value));
        }

        public static Color ambientLight
        {
            get => Get(r => ColorOf(r->ambient_flat));
            set => SetAmbient(r => Put(r->ambient_flat, value));
        }

        public static float ambientIntensity
        {
            get => Get(r => r->ambient_intensity);
            set => SetAmbient(r => r->ambient_intensity = Math.Clamp(Finite(value), 0f, 8f));
        }

        public static SphericalHarmonicsL2 ambientProbe
        {
            get => Get(r => ProbeOf(r));
            set => SetAmbient(r => PutProbe(r, value));
        }

        private static SphericalHarmonicsL2 ProbeOf(DisunityEnvironment* record)
        {
            var probe = new SphericalHarmonicsL2();
            for (var i = 0; i < 27; i++) probe[i / 9, i % 9] = record->ambient_probe[i];
            return probe;
        }

        private static void PutProbe(DisunityEnvironment* record, SphericalHarmonicsL2 probe)
        {
            for (var i = 0; i < 27; i++) record->ambient_probe[i] = Finite(probe[i / 9, i % 9]);
        }

        public static DefaultReflectionMode defaultReflectionMode
        {
            get => Get(r => (DefaultReflectionMode)r->reflection_mode);
            set => SetReflection(r => r->reflection_mode = (uint)value);
        }

        public static float reflectionIntensity
        {
            get => Get(r => r->reflection_intensity);
            set => SetReflection(r => r->reflection_intensity = Math.Clamp(Finite(value), 0f, 1f));
        }

        public static Texture? customReflectionTexture
        {
            get => Get(r => r->custom_kind == 0 ? null : _state.CustomReflection);
            set
            {
                var (kind, identity) = WorldGraphics.TextureOf(value);
                lock (Gate)
                {
                    SetReflection(r => (r->custom_kind, r->custom_texture) = (kind, identity));
                    _state.CustomReflection = WorldGraphics.IsUnbound(kind) ? null : value;
                }
            }
        }

        /// <summary>The cube baked from the skybox, which the default reflection reads in the skybox mode. A scene load sets it from
        /// the scene's render settings; scripts have no member for it.</summary>
        internal static Texture? generatedSkyboxReflection => Get(r => r->skybox_kind == 0 ? null : _state.SkyboxReflection);

        /// <summary>The cube the scene's default reflection reads, which is what <see cref="ReflectionProbe.defaultTexture"/> returns:
        /// the custom cube in the custom mode when one is set, and otherwise the cube baked from the skybox. It is null when the scene
        /// names neither. The host binds the same cube to a draw as <c>unity_SpecCube0</c>.</summary>
        internal static Texture? defaultReflectionTexture => Get(r =>
            (DefaultReflectionMode)r->reflection_mode == DefaultReflectionMode.Custom && !WorldGraphics.IsUnbound(r->custom_kind) ? _state.CustomReflection
            : WorldGraphics.IsUnbound(r->skybox_kind) ? null : _state.SkyboxReflection);

        /// <summary>The environment a scene's plan carries: its ambient light and default reflection, which a scene load writes to
        /// the world's record as one write.</summary>
        internal readonly record struct SceneEnvironment(AmbientMode AmbientMode, Color Sky, Color Equator, Color Ground, float AmbientIntensity,
            float[] Probe, DefaultReflectionMode ReflectionMode, float ReflectionIntensity, Texture? Custom, Texture? Skybox);

        /// <summary>Fills both sections of the world's record from a scene's plan. The flat colour is the sky colour, as a scene
        /// stores one ambient colour for both. The textures are stored with the host first, so the record names identities it holds.</summary>
        internal static void LoadEnvironment(SceneEnvironment scene)
        {
            var (custom, skybox) = (WorldGraphics.TextureOf(scene.Custom), WorldGraphics.TextureOf(scene.Skybox));
            lock (Gate)
            {
                var record = ReadRecord();
                record.ambient_set = 1;
                record.ambient_mode = (uint)scene.AmbientMode;
                record.ambient_intensity = Math.Clamp(Finite(scene.AmbientIntensity), 0f, 8f);
                Put(record.ambient_sky, scene.Sky);
                Put(record.ambient_flat, scene.Sky);
                Put(record.ambient_equator, scene.Equator);
                Put(record.ambient_ground, scene.Ground);
                for (var i = 0; i < 27; i++) record.ambient_probe[i] = Finite(scene.Probe[i]);
                record.reflection_set = 1;
                record.reflection_mode = (uint)scene.ReflectionMode;
                record.reflection_intensity = Math.Clamp(Finite(scene.ReflectionIntensity), 0f, 1f);
                (record.custom_kind, record.custom_texture) = custom;
                (record.skybox_kind, record.skybox_texture) = skybox;
                WriteRecord(&record);
                _state.CustomReflection = WorldGraphics.IsUnbound(custom.Kind) ? null : scene.Custom;
                _state.SkyboxReflection = WorldGraphics.IsUnbound(skybox.Kind) ? null : scene.Skybox;
            }
        }
    }
}
