// Equation from Krzysztof Narkowicz, "ACES Filmic Tone Mapping Curve" (2016-01-06):
// https://knarkowicz.wordpress.com/2016/01/06/aces-filmic-tone-mapping-curve/
// Coefficients are shared by CPU references and all generated shader sources.
const FIT: [f64; 5] = [2.51, 0.03, 2.43, 0.59, 0.14];

pub(crate) fn fitted(x: f64) -> f64 {
    let [
        num_quadratic,
        num_linear,
        den_quadratic,
        den_linear,
        den_constant,
    ] = FIT;
    x * (num_quadratic * x + num_linear) / (x * (den_quadratic * x + den_linear) + den_constant)
}

pub(crate) fn shader(source: &str) -> std::borrow::Cow<'static, str> {
    let [
        num_quadratic,
        num_linear,
        den_quadratic,
        den_linear,
        den_constant,
    ] = FIT;
    format!(
        "const ACES_A: f32 = {num_quadratic};\nconst ACES_B: f32 = {num_linear};\nconst ACES_C: f32 = {den_quadratic};\nconst ACES_D: f32 = {den_linear};\nconst ACES_E: f32 = {den_constant};\n\
         fn aces_fitted(x: f32) -> f32 {{\n\
         return x * (ACES_A * x + ACES_B) / (x * (ACES_C * x + ACES_D) + ACES_E);\n}}\n\
         fn aces_display(x: f32) -> f32 {{\n\
         if x >= 16.0 {{ return 1.0; }}\n\
         return clamp(aces_fitted(x), 0.0, 1.0);\n}}\n{source}"
    ).into()
}
