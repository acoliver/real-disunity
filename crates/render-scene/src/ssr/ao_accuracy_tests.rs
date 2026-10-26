// Bounds for the fixtures in fidelity_tests, from WGSL §15.7.4 and §15.7.6:
// https://www.w3.org/TR/WGSL/#accuracy-of-builtin-functions
// exp/exp2: (3 + 2|x|) ULP; pow: exp2(y * log2(x)); log2 on
// [0.5, 2]: absolute error 2^-21. +, -, * are correctly rounded;
// / permits 2.5 ULP. Use u=2^-23, an upper bound on relative f32 ULP,
// also conservatively covering either permitted arithmetic rounding direction.
// All fixture depths, AO taps, cosine, roughness, factors and confidence are
// binary-exact in their input f32/R32Float/R16Float/Rgba16Float storage, so
// their input quantisation error is zero. 1.01's f32 literal error is explicit.
// No intermediate AO is stored. Output Rgba16Float conversion is bounded by
// one binary16 ULP (relative <=2^-10), covering both adjacent representable values.
// Host uploads use half::f16 round-to-nearest, ties-to-even, tested separately.
const U: f64 = 1.0 / 8_388_608.0;

pub(super) fn half_value(bytes: &[u8]) -> f64 {
    f64::from(::half::f16::from_bits(u16::from_le_bytes(bytes[..2].try_into().unwrap())).to_f32())
}

pub(super) fn storage_error(value: f64) -> f64 {
    value.abs() / 1024.0 + 2.0f64.powi(-24)
}

pub(super) fn raw_oracle() -> (f64, f64) {
    let weights = [0.0f64, -1.0, -2.0, -3.0].map(f64::exp);
    let raw = weights[3] / weights.iter().sum::<f64>();
    // Each weight has relative error <=9u. Numerator dot with [0,0,0,1]
    // is exact; denominator's three additions contribute (1-u)^-3.
    let ratio = (1.0 + 9.0 * U) * (1.0 + 2.5 * U) / ((1.0 - 9.0 * U) * (1.0 - U).powi(3));
    (raw, raw * (ratio - 1.0))
}

pub(super) fn visibility_oracle() -> (f64, f64) {
    let (raw, raw_error) = raw_oracle();
    let ao = raw * 1.01;
    let gain_error = (f64::from(1.01f32) - 1.01).abs();
    let ao_error = raw_error * (1.01 + gain_error)
        + raw * gain_error
        + (raw + raw_error) * (1.01 + gain_error) * U;
    // min with the exact material ceiling 1 is non-expansive.
    let base = 1.0 + ao;
    let base_error = ao_error + (base + ao_error) * U;
    let exponent = 2.0f64.powi(-9);
    // -16*0.5-1 is exact; exp2(-9) allows 21 ULP.
    let exponent_error = exponent * 21.0 * U;
    let log = base.log2();
    let log_error = base_error / ((base - base_error) * std::f64::consts::LN_2) + 2.0f64.powi(-21);
    let product = exponent * log;
    let product_error = exponent_error * log.abs()
        + (exponent + exponent_error) * log_error
        + (exponent + exponent_error) * (log.abs() + log_error) * U;
    let power = product.exp2();
    // exp2 is monotone. Propagate input error before its own ULP allowance.
    let power_high = (product + product_error).exp2();
    let power_error =
        power_high - power + power_high * (3.0 + 2.0 * (product.abs() + product_error)) * U;
    let visibility = power - 1.0 + ao;
    let error = power_error
        + ao_error
        + (power + power_error + 1.0) * U
        + (visibility.abs() + power_error + ao_error) * U;
    (visibility.clamp(0.0, 1.0), error)
}

pub(super) fn assert_resolve(actual: f64, scene: f64, environment: f64, confidence: f64) {
    let (visibility, visibility_error) = visibility_oracle();
    let expected = scene + confidence * (0.5 * visibility - environment);
    // factor*visibility, incident*factor, environment*confidence, subtraction,
    // and additive blending: six arithmetic operations, bounded by absolute
    // magnitudes to retain cancellation error. Reflection mix is exact here:
    // every level and every tap has the same binary-exact value.
    let arithmetic = 6.0 * U * (scene.abs() + 0.5 + environment.abs());
    let before_storage = confidence * 0.5 * visibility_error + arithmetic;
    let bound = before_storage + storage_error(expected.abs() + before_storage);
    assert!(
        bound < 0.001,
        "fixture bound must distinguish lobe errors: {bound}"
    );
    assert!(
        (actual - expected).abs() <= bound,
        "actual {actual}, oracle {expected}, derived bound {bound}"
    );
}
