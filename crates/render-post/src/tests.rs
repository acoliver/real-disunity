use super::*;

#[test]
fn authored_curve_coefficients_are_not_replaced_with_a_display_look() {
    let identity = RationalCurve {
        numerator: [0.0, 1.0, 0.0],
        denominator: [0.0, 0.0, 1.0],
    };
    for value in [0.0, 0.25, 1.0, 8.0] {
        assert!((identity.evaluate(value) - value).abs() < 1e-6);
    }
    let curve = RationalCurve {
        numerator: [1.0, 2.0, 3.0],
        denominator: [4.0, 5.0, 6.0],
    };
    assert!((curve.evaluate(2.0) - 11.0 / 32.0).abs() < 1e-6);
}

#[test]
fn composed_post_shader_is_valid() {
    let module = naga::front::wgsl::parse_str(WGSL).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}
