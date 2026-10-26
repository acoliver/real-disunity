// Test-only equations: Heitz (2014), height-correlated Smith lambda;
// Karis (2013), GGX inversion and split-sum integration. No production kernels.
pub fn integral(no_v: f32, roughness: f32, samples: u32, schlick: bool) -> [f64; 2] {
    let nv = f64::from(no_v);
    let r = f64::from(roughness);
    let alpha2 = r.powi(4);
    let vx = (1.0 - nv * nv).sqrt();
    let mut sum = [0.0; 2];
    for i in 0..samples {
        let u = f64::from(i) / f64::from(samples);
        let v = f64::from(i.reverse_bits()) / 4_294_967_296.0;
        let hz = ((1.0 - v) / (1.0 + (alpha2 - 1.0) * v)).sqrt();
        let hx = (1.0 - hz * hz).max(0.0).sqrt() * (std::f64::consts::TAU * u).cos();
        let vh = (vx * hx + nv * hz).max(0.0);
        let nl = 2.0 * vh * hz - nv;
        if nl > 0.0 {
            let g = if schlick {
                let k = (r * r / 2.0).max(1e-5);
                nv / (nv * (1.0 - k) + k) * nl / (nl * (1.0 - k) + k)
            } else {
                let lambda_v = ((1.0 + alpha2 * (1.0 / (nv * nv) - 1.0)).sqrt() - 1.0) / 2.0;
                let lambda_l = ((1.0 + alpha2 * (1.0 / (nl * nl) - 1.0)).sqrt() - 1.0) / 2.0;
                1.0 / (1.0 + lambda_v + lambda_l)
            };
            let weight = g * vh / (nv * hz);
            let fc = (1.0 - vh).powi(5);
            sum[0] += (1.0 - fc) * weight;
            sum[1] += fc * weight;
        }
    }
    sum.map(|v| v / f64::from(samples))
}
