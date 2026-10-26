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
    let matrix = matrix_shader();
    let grading = include_str!("display_grading.wgsl");
    format!(
        "const ACES_A: f32 = {num_quadratic};\nconst ACES_B: f32 = {num_linear};\nconst ACES_C: f32 = {den_quadratic};\nconst ACES_D: f32 = {den_linear};\nconst ACES_E: f32 = {den_constant};\n\
         fn aces_fitted(x: f32) -> f32 {{\n\
         return x * (ACES_A * x + ACES_B) / (x * (ACES_C * x + ACES_D) + ACES_E);\n}}\n\
         fn aces_display(x: f32) -> f32 {{\n\
         if x >= 16.0 {{ return 1.0; }}\n\
         return clamp(aces_fitted(x), 0.0, 1.0);\n}}\n{matrix}
{grading}
{source}"
    ).into()
}

// Stephen Hill's ACES fit, BakingLab, MIT licensed:
// https://github.com/TheRealMJP/BakingLab/blob/master/BakingLab/ACES.hlsl
// Copyright (c) 2016 MJP. Matrix entries are rows, on both CPU and GPU.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
const INPUT: [[f64; 3]; 3] = [
    [0.59719, 0.35458, 0.04823],
    [0.07600, 0.90834, 0.01566],
    [0.02840, 0.13383, 0.83777],
];
const OUTPUT: [[f64; 3]; 3] = [
    [1.60475, -0.53108, -0.07367],
    [-0.10208, 1.10813, -0.00605],
    [-0.00327, -0.07276, 1.07602],
];
const RATIONAL: [f64; 5] = [0.024_578_6, -0.000_090_537, 0.983_729, 0.432_951, 0.238_081];
fn transform(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    m.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
}
pub(crate) fn matrix_fit(rgb: [f64; 3]) -> [f64; 3] {
    let [
        num_linear,
        num_constant,
        den_square,
        den_linear,
        den_constant,
    ] = RATIONAL;
    let rgb = rgb.map(|v| v.min(1e18));
    let fitted = transform(INPUT, rgb).map(|x| {
        (x * (x + num_linear) + num_constant) / (x * (den_square * x + den_linear) + den_constant)
    });
    transform(OUTPUT, fitted).map(|v| v.clamp(0.0, 1.0))
}
fn matrix_shader() -> String {
    use std::fmt::Write;
    let mut source = String::new();
    for (name, matrix) in [("hill_input", INPUT), ("hill_output", OUTPUT)] {
        write!(
            source,
            "fn {name}(v: vec3<f32>) -> vec3<f32> {{ return vec3<f32>("
        )
        .unwrap();
        for (i, row) in matrix.iter().enumerate() {
            if i > 0 {
                source.push(',');
            }
            write!(source, "dot(vec3<f32>({},{},{}),v)", row[0], row[1], row[2]).unwrap();
        }
        source.push_str("); }\n");
    }
    let [
        num_linear,
        num_constant,
        den_square,
        den_linear,
        den_constant,
    ] = RATIONAL;
    writeln!(source, "fn hill_display(rgb: vec3<f32>) -> vec3<f32> {{
        let x = hill_input(min(rgb, vec3<f32>(1e18)));
        let v = (x * (x + vec3<f32>({num_linear})) + vec3<f32>({num_constant})) / (x * ({den_square} * x + vec3<f32>({den_linear})) + vec3<f32>({den_constant}));
        return clamp(hill_output(v), vec3<f32>(0.0), vec3<f32>(1.0)); }}").unwrap();
    source
}
