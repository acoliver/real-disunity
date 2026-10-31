namespace UnityEngine;

public sealed partial class ParticleSystem
{
    /// <summary>
    /// A value that varies over a particle's life or between particles: a constant, two constants, one curve or two
    /// curves, each curve scaled by <see cref="curveMultiplier"/>. A mode that picks between two values uses a
    /// random factor from <see cref="Random.value"/> in <see cref="Evaluate(float)"/> and the given factor in
    /// <see cref="Evaluate(float, float)"/>. A value built from a float is a constant.
    /// </summary>
    public struct MinMaxCurve
    {
        private ParticleSystemCurveMode _mode;
        private float _constant;
        private float _constantMin;
        private float _constantMax;
        private float _multiplier;
        private AnimationCurve? _curve;
        private AnimationCurve? _curveMin;
        private AnimationCurve? _curveMax;

        public MinMaxCurve(float constant)
        {
            this = default;
            _constant = constant;
            _multiplier = 1f;
        }

        public MinMaxCurve(float min, float max)
        {
            this = default;
            _mode = ParticleSystemCurveMode.TwoConstants;
            (_constantMin, _constantMax, _multiplier) = (min, max, 1f);
        }

        public MinMaxCurve(float multiplier, AnimationCurve curve)
        {
            this = default;
            _mode = ParticleSystemCurveMode.Curve;
            (_multiplier, _curve) = (multiplier, curve);
        }

        public MinMaxCurve(float multiplier, AnimationCurve min, AnimationCurve max)
        {
            this = default;
            _mode = ParticleSystemCurveMode.TwoCurves;
            (_multiplier, _curveMin, _curveMax) = (multiplier, min, max);
        }

        public ParticleSystemCurveMode mode { readonly get => _mode; set => _mode = value; }

        public float constant { readonly get => _constant; set => _constant = value; }

        public float constantMin { readonly get => _constantMin; set => _constantMin = value; }

        public float constantMax { readonly get => _constantMax; set => _constantMax = value; }

        public float curveMultiplier { readonly get => _multiplier; set => _multiplier = value; }

        /// <summary>The curve of <see cref="ParticleSystemCurveMode.Curve"/>, null in the other modes.</summary>
        public AnimationCurve? curve { readonly get => _mode == ParticleSystemCurveMode.Curve ? _curve : null; set => _curve = value; }

        /// <summary>The lower curve of <see cref="ParticleSystemCurveMode.TwoCurves"/>, null in the other modes.</summary>
        public AnimationCurve? curveMin { readonly get => _mode == ParticleSystemCurveMode.TwoCurves ? _curveMin : null; set => _curveMin = value; }

        /// <summary>The upper curve of <see cref="ParticleSystemCurveMode.TwoCurves"/>, null in the other modes.</summary>
        public AnimationCurve? curveMax { readonly get => _mode == ParticleSystemCurveMode.TwoCurves ? _curveMax : null; set => _curveMax = value; }

        public readonly float Evaluate(float time) => Evaluate(time, Random.value);

        public readonly float Evaluate(float time, float lerpFactor) => _mode switch
        {
            ParticleSystemCurveMode.Constant => _constant,
            ParticleSystemCurveMode.TwoConstants => Mathf.Lerp(_constantMin, _constantMax, lerpFactor),
            ParticleSystemCurveMode.Curve => (_curve?.Evaluate(time) ?? 0f) * _multiplier,
            ParticleSystemCurveMode.TwoCurves =>
                Mathf.Lerp(_curveMin?.Evaluate(time) ?? 0f, _curveMax?.Evaluate(time) ?? 0f, lerpFactor) * _multiplier,
            _ => throw new NotSupportedException($"MinMaxCurve.Evaluate: unknown mode {_mode}"),
        };

        public static implicit operator MinMaxCurve(float constant) => new(constant);
    }
}
