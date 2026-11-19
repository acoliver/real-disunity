use super::*;

#[test]
fn unix_fields_preserve_zero_and_the_full_origin() {
    assert_eq!(unix_duration(0, 0).unwrap(), Duration::ZERO);
    assert_eq!(
        unix_duration(12, 999_999_999).unwrap(),
        Duration::new(12, 999_999_999)
    );
    assert_eq!(
        unix_duration(i128::from(u64::MAX), 999_999_999).unwrap(),
        Duration::MAX
    );
}

#[test]
fn unix_fields_reject_negative_and_unnormalized_values() {
    for (seconds, nanos) in [
        (-1, 0),
        (0, -1),
        (0, 1_000_000_000),
        (0, i128::MAX),
        (i128::MAX, 0),
    ] {
        assert_eq!(
            unix_duration(seconds, nanos).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn unix_status_preserves_the_supplied_os_error_without_reading_on_success() {
    unix_status(0, || panic!("successful status needs no error read")).unwrap();
    let error = unix_status(-1, || io::Error::from_raw_os_error(13)).unwrap_err();
    assert_eq!(error.raw_os_error(), Some(13));
}

#[test]
fn mach_status_preserves_the_mach_failure_code() {
    mach_status(0).unwrap();
    let error = mach_status(5).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert!(error.to_string().contains("mach_timebase_info"));
    assert!(error.to_string().contains("Mach status 5"));
}

#[test]
fn darwin_requires_both_ratio_terms_to_be_positive() {
    for (numer, denom) in [(0, 0), (0, 1), (1, 0)] {
        assert_eq!(
            Timebase::new(numer, denom).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn darwin_converts_whole_raw_ticks_with_fractional_flooring() {
    let timebase = Timebase::new(125, 3).unwrap();
    assert_eq!(timebase.duration(0).unwrap(), Duration::ZERO);
    assert_eq!(timebase.duration(1).unwrap(), Duration::from_nanos(41));
    assert_eq!(
        timebase.duration(24_000_000).unwrap(),
        Duration::from_secs(1)
    );
    let mut previous = Duration::ZERO;
    for ticks in [0, 1, 1, 2, 3, 999, u64::MAX] {
        let sample = timebase.duration(ticks).unwrap();
        assert!(sample >= previous);
        previous = sample;
    }
}

#[test]
fn darwin_maximum_ticks_use_wide_intermediates() {
    let unit = Timebase::new(1, 1).unwrap();
    assert_eq!(
        unit.duration(u64::MAX).unwrap(),
        Duration::from_nanos(u64::MAX)
    );
    let wide = Timebase::new(1_000_000_000, 1).unwrap();
    let nanos = u128::from(u64::MAX) * 1_000_000_000;
    let result = wide.duration(u64::MAX).unwrap();
    assert_eq!(result.as_nanos(), nanos);
    assert_eq!(
        Timebase::new(u32::MAX, 1)
            .unwrap()
            .duration(u64::MAX)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        Timebase::new(1, u32::MAX)
            .unwrap()
            .duration(u64::MAX)
            .unwrap()
            .as_nanos(),
        u128::from(u64::MAX) / u128::from(u32::MAX)
    );
}

#[test]
fn darwin_duration_conversion_rejects_overflow() {
    assert_eq!(
        duration_from_nanos(Duration::MAX.as_nanos()).unwrap(),
        Duration::MAX
    );
    for nanos in [Duration::MAX.as_nanos() + 1, u128::MAX] {
        assert_eq!(
            duration_from_nanos(nanos).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[test]
fn unsupported_targets_report_an_explicit_error() {
    let error = unsupported();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert!(error.to_string().contains("kernel monotonic input"));
}
