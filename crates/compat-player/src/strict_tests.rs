use super::*;
use serde_json::json;

#[test]
fn profile_requires_exact_explicit_opt_in() {
    assert!(!selected(&json!({})).unwrap());
    assert!(selected(&json!({"execution_profile":"native_phase_context_v1"})).unwrap());
    for value in [json!(null), json!(true), json!("native_scheduler_v1")] {
        assert!(selected(&json!({"execution_profile":value})).is_err());
    }
}

#[test]
fn ratios_accept_exact_integer_components_only() {
    assert_eq!(ratio(&json!({"num":1,"den":60})).unwrap(), (1, 60));
    assert_eq!(
        ratio(&json!({"num":1,"den":1_000_000_000_000_u64}))
            .unwrap()
            .1,
        1_000_000_000_000
    );
    for value in [
        json!(0.1),
        json!({"num":1.0,"den":60}),
        json!({"num":1,"den":0}),
        json!({"num":-1,"den":60}),
        json!({"num":1,"den":60,"extra":0}),
        json!({"num":9_007_199_254_740_992_u64,"den":1}),
    ] {
        assert!(ratio(&value).is_err(), "{value}");
    }
}

#[test]
fn duration_chunks_conserve_every_nanosecond_and_reject_stalls() {
    for nanos in [0, 1, 16_666_666, 16_666_667, 100_000_001, 249_999_990] {
        let duration = Duration::from_nanos(nanos);
        let chunks = steps(duration).unwrap();
        assert_eq!(chunks.iter().sum::<Duration>(), duration);
        assert!(chunks.len() <= 15);
        assert!(
            chunks
                .iter()
                .all(|chunk| !chunk.is_zero() && chunk.as_nanos() <= 16_666_666)
        );
    }
    assert!(steps(Duration::from_nanos(249_999_991)).is_err());
    assert!(steps(Duration::from_secs(4)).is_err());
}

fn key(id: u64, pressed: bool) -> Value {
    json!({"event_id":id,"timestamp":{"num":1,"den":1000},"source":"replay",
        "kind":"key","key":"W","pressed":pressed})
}

#[test]
fn normalized_short_tap_retains_ids_ties_order_and_case() {
    let input = json!([
        key(9_007_199_254_740_993, true),
        key(9_007_199_254_740_995, false)
    ]);
    assert_eq!(events(&input).unwrap(), input);
}

#[test]
fn malformed_normalized_batches_reject_as_a_whole() {
    for input in [
        json!([key(2, true), key(1, false)]),
        json!([key(1, true), key(1, false)]),
        json!([key(0, true)]),
        json!({}),
        json!([{"kind":"focus","focused":true}]),
    ] {
        assert!(events(&input).is_err(), "{input}");
    }
    let mut invalid = key(2, false);
    invalid["repeat"] = json!(true);
    assert!(events(&json!([key(1, true), invalid])).is_err());
    assert!(events(&json!(vec![key(1, true); 4097])).is_err());
}
