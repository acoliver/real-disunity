pub(super) fn reference_srgb8(value: f32) -> u8 {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    if value >= 1.0 {
        return 255;
    }
    let encoded = if f64::from(value) <= 0.003_130_8 {
        f64::from(value) * 12.92
    } else {
        1.055 * f64::from(value).powf(1.0 / 2.4) - 0.055
    };
    exact_byte(encoded.mul_add(255.0, 0.5).floor())
}

fn exact_byte(value: f64) -> u8 {
    (0..=u8::MAX)
        .find(|&code| f64::from(code).to_bits() == value.to_bits())
        .expect("quantized oracle result is an integer byte")
}

pub(super) fn reference_alpha8(value: f32) -> u8 {
    if value.is_nan() || value <= 0.0 {
        0
    } else if value >= 1.0 {
        255
    } else {
        exact_byte((f64::from(value) * 255.0 + 0.5).floor())
    }
}

pub(super) fn nearest_unit_f32(value: f64) -> f32 {
    assert!((0.0..=1.0).contains(&value));
    let (mut low, mut high) = (0, 1.0_f32.to_bits());
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        if f64::from(f32::from_bits(middle)) <= value {
            low = middle;
        } else {
            high = middle;
        }
    }
    let below = value - f64::from(f32::from_bits(low));
    let above = f64::from(f32::from_bits(high)) - value;
    let distance_order = below.total_cmp(&above);
    f32::from_bits(
        if distance_order.is_lt() || (distance_order.is_eq() && low % 2 == 0) {
            low
        } else {
            high
        },
    )
}

pub(super) fn threshold(k: u32) -> f32 {
    let encoded = (f64::from(k) + 0.5) / 255.0;
    let linear = if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    };
    nearest_unit_f32(linear)
}

#[test]
fn independent_oracle_covers_every_threshold_and_adjacent_f32_values() {
    let mut cases = vec![
        0.0,
        -0.0,
        0.5,
        0.25,
        0.125,
        f32::from_bits(1),
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NAN,
        -1.0,
        1.0,
        f32::from_bits(1.0_f32.to_bits() - 1),
    ];
    for k in 0..255 {
        let encoded = (f64::from(k) + 0.5) / 255.0;
        let mathematical = if encoded <= 0.04045 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        };
        let boundary = threshold(k);
        for value in [
            f32::from_bits(boundary.to_bits() - 1),
            boundary,
            f32::from_bits(boundary.to_bits() + 1),
        ] {
            cases.push(value);
            let expected = if f64::from(value) > mathematical {
                k + 1
            } else {
                k
            };
            assert_eq!(reference_srgb8(value), u8::try_from(expected).unwrap());
        }
    }
    assert_eq!(reference_srgb8(f32::NAN), 0);
    assert_eq!(reference_srgb8(-0.0), 0);
    assert!(cases.len() >= 255 * 3);
}

#[test]
fn exact_shader_parses_and_validates_as_wgsl() {
    let source = format!(
        "{}{}",
        include_str!("copy.wgsl"),
        include_str!("exact.wgsl")
    );
    let module = naga::front::wgsl::parse_str(&source).expect("exact copy WGSL parses");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("exact copy WGSL validates");
}
