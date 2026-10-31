namespace UnityEngine;

public sealed partial class ParticleSystem
{
    /// <summary>
    /// One particle's state. <see cref="rotation"/> and <see cref="angularVelocity"/> are the z components of
    /// <see cref="rotation3D"/> and <see cref="angularVelocity3D"/>, in degrees. A new particle has a lifetime of
    /// 0, size 1 and a white start color.
    /// </summary>
    public struct Particle
    {
        private Vector3 _startSize;
        private Color32 _startColor;
        private bool _initialized;

        public Vector3 position { get; set; }

        public Vector3 velocity { get; set; }

        public float startLifetime { get; set; }

        public float remainingLifetime { get; set; }

        public uint randomSeed { get; set; }

        public Vector3 rotation3D { get; set; }

        public Vector3 angularVelocity3D { get; set; }

        public Vector3 axisOfRotation { get; set; }

        public Vector3 startSize3D
        {
            readonly get => _initialized ? _startSize : Vector3.one;
            set => (_startSize, _initialized) = (value, true);
        }

        public float startSize
        {
            readonly get => startSize3D.x;
            set => startSize3D = new Vector3(value, value, value);
        }

        public Color32 startColor
        {
            readonly get => _initialized ? _startColor : new Color32(255, 255, 255, 255);
            set => (_startColor, _initialized) = (value, true);
        }

        public float rotation
        {
            readonly get => rotation3D.z;
            set => rotation3D = new Vector3(rotation3D.x, rotation3D.y, value);
        }

        public float angularVelocity
        {
            readonly get => angularVelocity3D.z;
            set => angularVelocity3D = new Vector3(angularVelocity3D.x, angularVelocity3D.y, value);
        }

        public readonly Vector3 totalVelocity => velocity;

        /// <summary>The start size times the system's size over lifetime curve at this particle's age, when that module is enabled.</summary>
        public readonly float GetCurrentSize(ParticleSystem system) => GetCurrentSize3D(system).x;

        public readonly Vector3 GetCurrentSize3D(ParticleSystem system)
        {
            ArgumentNullException.ThrowIfNull(system);
            var module = system.sizeOverLifetime;
            return module.enabled ? startSize3D * module.size.Evaluate(Age, Fixed(randomSeed)) : startSize3D;
        }

        /// <summary>The start color times the system's color over lifetime at this particle's age, when that module is enabled.</summary>
        public readonly Color32 GetCurrentColor(ParticleSystem system)
        {
            ArgumentNullException.ThrowIfNull(system);
            var module = system.colorOverLifetime;
            Color start = startColor;
            return module.enabled ? start * module.color.Evaluate(Age, Fixed(randomSeed)) : start;
        }

        private readonly float Age => startLifetime > 0f ? 1f - remainingLifetime / startLifetime : 0f;

        /// <summary>The random factor of a particle: its seed spread over [0, 1], so the same particle always evaluates the same.</summary>
        private static float Fixed(uint seed) => (seed % 1000u) / 999f;
    }
}
