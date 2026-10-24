use super::parse;

#[test]
fn duplicate_evidence_fields_are_rejected() {
    for bytes in [
        br#"{"disposition":"UNQUALIFIED","disposition":"ready"}"#.as_slice(),
        br#"{"sources":{"A.cs":"a","A.cs":"b"}}"#.as_slice(),
    ] {
        assert!(parse(bytes).is_err());
    }
}

#[test]
fn deeply_nested_evidence_is_rejected() {
    let text = format!("{}0{}", "[".repeat(34), "]".repeat(34));
    assert!(parse(text.as_bytes()).is_err());
}

#[test]
fn trailing_evidence_is_rejected() {
    assert!(parse(b"{} {}").is_err());
}

#[test]
fn inspection_diagnostics_preserve_exact_signatures_and_failures() {
    let input =
        br#"{"signature":"header=20;arity=1;Void(Int32&)","failures":[{"token":"0x0a000001"}]}"#;
    let value = parse(input).expect("diagnostic JSON");
    assert_eq!(value["signature"], "header=20;arity=1;Void(Int32&)");
    assert_eq!(value["failures"][0]["token"], "0x0a000001");
}
