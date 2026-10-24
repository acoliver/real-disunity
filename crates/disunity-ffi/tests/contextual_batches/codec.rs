use super::*;
#[test]
fn exact_identity_strings_unknown_fields_depth_and_float_bit_validation() {
    let w = World::new();
    let (_, c, _) = setup(&w, 1);
    let e = batch(&w, &c);
    let before = inspect(&w, &c, &e);
    for id in [
        json!(9_007_199_254_740_992_u64),
        json!("18446744073709551616"),
        json!("01"),
        json!("-1"),
        json!("1e0"),
        json!(" 1"),
        json!(0),
        json!("18446744073709551615"),
    ] {
        let mut bad = e.clone();
        bad["callbacks"][1]["binding"] = id;
        assert_eq!(
            w.call(json!({"op":"callbacks.accept","context":c,"envelope":bad}))["ok"],
            false
        );
    }
    let mut negative_zero = e.clone();
    negative_zero["context"]["dynamic_delta"] = json!(-0.0);
    assert_eq!(
        w.call(json!({"op":"callbacks.accept","context":c,"envelope":negative_zero}))["ok"],
        false
    );
    let raw =
        serde_json::to_string(&json!({"op":"callbacks.accept","context":c,"envelope":e})).unwrap();
    let deep = format!("{}0{}", "[".repeat(65), "]".repeat(65));
    let bad = raw.replacen("\"payload\":{}", &format!("\"payload\":{deep}"), 1);
    assert_eq!(support::submit(w.0, bad.as_bytes()), (4, 0));
    assert_eq!(inspect(&w, &c, &e), before);
    let mut strings = e.clone();
    for record in strings["callbacks"].as_array_mut().unwrap() {
        for key in ["binding", "component"] {
            record[key] = json!(record[key].as_u64().unwrap().to_string());
        }
    }
    accept(&w, &c, &strings);
    assert_eq!(enter(&w, &c, &e, 0)["execute"], true);
}
#[test]
fn cleanup_only_descriptor_and_initialize_registration_guards_are_explicit() {
    let w = World::new();
    let d = ok(&w, json!({"op":"describe"}));
    let scoped = &d["native_contextual_lifecycle_batch"];
    assert_eq!(scoped["managed_integration"], false);
    assert_eq!(scoped["callback_batch_attestation"], false);
    assert_eq!(scoped["batch_phases"], json!(["Cleanup"]));
    let s = ok(&w, json!({"op":"session.open","profile":PROFILE}));
    assert_eq!(s["supported_phases"], json!(["Initialize", "Cleanup"]));
    let host = json!({"session":s["session"],"host_key":s["host_key"]});
    let c = claim(&w, &host, "Initialize");
    let o = ok(&w, json!({"op":"object.create"}))["handle"].clone();
    let component =
        ok(&w, json!({"op":"component.add","object":o,"kind":"Script"}))["handle"].clone();
    assert_eq!(w.call(json!({"op":"readiness.register","context":c,"component":component,"callbacks":["Update"]}))["ok"], false);
    assert_eq!(
        ok(
            &w,
            json!({"op":"readiness.register","context":c,"component":component,"callbacks":[]})
        )["binding"],
        component
    );
    assert_eq!(
        w.call(json!({"op":"callbacks.next","context":c}))["ok"],
        false
    );
}
