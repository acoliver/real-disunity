/// Quantize rational byte values, rejecting backend-dependent rounding ties.
pub fn quantize(numerators: [u16; 4], denominator: u16) -> [u8; 4] {
    numerators.map(|value| {
        let scaled = f32::from(value) / f32::from(denominator);
        assert!((scaled.fract() - 0.5).abs() >= 0.1);
        u8::try_from((value + denominator / 2) / denominator).unwrap()
    })
}
