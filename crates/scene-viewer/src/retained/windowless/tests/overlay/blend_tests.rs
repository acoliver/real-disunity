use super::assert_blended_within_one;

#[test]
fn blended_tolerance_accepts_one_byte_in_either_direction() {
    assert_blended_within_one(&[187, 189, 0, 255], [188, 188, 0, 255]);
    assert_blended_within_one(&[0, 255, 127, 254], [0, 255, 128, 255]);
}

#[test]
fn blended_tolerance_rejects_two_bytes_in_every_channel() {
    for channel in 0..4 {
        for actual in [126, 130] {
            let mut pixel = [128; 4];
            pixel[channel] = actual;
            assert!(
                std::panic::catch_unwind(|| assert_blended_within_one(&pixel, [128; 4])).is_err()
            );
        }
    }
}
