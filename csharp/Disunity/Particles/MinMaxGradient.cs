namespace UnityEngine;

public sealed partial class ParticleSystem
{
    /// <summary>
    /// A color that varies over a particle's life or between particles: one color, two colors, a gradient, two
    /// gradients or a random color of a gradient. A mode that picks between two values uses a random factor from
    /// <see cref="Random.value"/> in <see cref="Evaluate(float)"/> and the given factor in
    /// <see cref="Evaluate(float, float)"/>. A value built from a <see cref="Color"/> is that color and one built
    /// from a <see cref="Gradient"/> is that gradient.
    /// </summary>
    public struct MinMaxGradient
    {
        private ParticleSystemGradientMode _mode;
        private Color _color;
        private Color _colorMin;
        private Color _colorMax;
        private Gradient? _gradient;
        private Gradient? _gradientMin;
        private Gradient? _gradientMax;

        public MinMaxGradient(Color color)
        {
            this = default;
            _color = color;
        }

        public MinMaxGradient(Color min, Color max)
        {
            this = default;
            _mode = ParticleSystemGradientMode.TwoColors;
            (_colorMin, _colorMax) = (min, max);
        }

        public MinMaxGradient(Gradient gradient)
        {
            this = default;
            _mode = ParticleSystemGradientMode.Gradient;
            _gradient = gradient;
        }

        public MinMaxGradient(Gradient min, Gradient max)
        {
            this = default;
            _mode = ParticleSystemGradientMode.TwoGradients;
            (_gradientMin, _gradientMax) = (min, max);
        }

        public ParticleSystemGradientMode mode { readonly get => _mode; set => _mode = value; }

        public Color color { readonly get => _color; set => _color = value; }

        public Color colorMin { readonly get => _colorMin; set => _colorMin = value; }

        public Color colorMax { readonly get => _colorMax; set => _colorMax = value; }

        /// <summary>The gradient of the gradient and random color modes, null in the others.</summary>
        public Gradient? gradient
        {
            readonly get => _mode is ParticleSystemGradientMode.Gradient or ParticleSystemGradientMode.RandomColor ? _gradient : null;
            set => _gradient = value;
        }

        public Gradient? gradientMin { readonly get => _mode == ParticleSystemGradientMode.TwoGradients ? _gradientMin : null; set => _gradientMin = value; }

        public Gradient? gradientMax { readonly get => _mode == ParticleSystemGradientMode.TwoGradients ? _gradientMax : null; set => _gradientMax = value; }

        public readonly Color Evaluate(float time) => Evaluate(time, Random.value);

        public readonly Color Evaluate(float time, float lerpFactor) => _mode switch
        {
            ParticleSystemGradientMode.Color => _color,
            ParticleSystemGradientMode.TwoColors => Color.Lerp(_colorMin, _colorMax, lerpFactor),
            ParticleSystemGradientMode.Gradient => (_gradient ?? new Gradient()).Evaluate(time),
            ParticleSystemGradientMode.TwoGradients =>
                Color.Lerp((_gradientMin ?? new Gradient()).Evaluate(time), (_gradientMax ?? new Gradient()).Evaluate(time), lerpFactor),
            ParticleSystemGradientMode.RandomColor => (_gradient ?? new Gradient()).Evaluate(lerpFactor),
            _ => throw new NotSupportedException($"MinMaxGradient.Evaluate: unknown mode {_mode}"),
        };

        public static implicit operator MinMaxGradient(Color color) => new(color);

        public static implicit operator MinMaxGradient(Gradient gradient) => new(gradient);
    }
}
