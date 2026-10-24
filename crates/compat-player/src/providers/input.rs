use super::binding::Binding;
use serde_json::{Value, json};
use worldrunner::managed::CompatibilityHost;

pub(super) fn request(
    host: &CompatibilityHost,
    world: u64,
    value: impl Into<Value>,
) -> Result<Value, String> {
    host.request(world, &value.into())
        .map_err(|e| e.to_string())
}
fn sample(host: &CompatibilityHost, member: &str) -> Result<Value, String> {
    let response = host
        .observe(&json!({"op":"managed.public","type":"ProviderKeyboardProbe","path":member}))
        .map_err(|e| e.to_string())?;
    serde_json::from_str(
        response["data"]
            .as_str()
            .ok_or("keyboard probe returned no sample")?,
    )
    .map_err(|e| e.to_string())
}
pub(super) fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(format!("provider qualification: {message}"))
    }
}
fn key(id: u64, time: u64, name: &str, pressed: bool) -> Value {
    json!({"event_id":id,"timestamp":{"num":time,"den":1000},"source":"replay","kind":"key","key":name,"pressed":pressed})
}
fn events(host: &CompatibilityHost, world: u64, events: impl Into<Value>) -> Result<(), String> {
    let events = events.into();
    request(host, world, json!({"op":"input.events","events":events}))?;
    Ok(())
}
fn frame(host: &mut CompatibilityHost, world: u64) -> Result<(Value, Value), String> {
    host.advance_rational(world, 1, 50)
        .map_err(|e| e.to_string())?;
    Ok((sample(host, "DynamicSample")?, sample(host, "FixedSample")?))
}
pub(super) fn qualify(host: &mut CompatibilityHost, binding: &Binding) -> Result<Value, String> {
    let world = host.create_world().map_err(|e| e.to_string())?;
    let result = run(host, binding, world);
    let unload = host.unload().map_err(|e| e.to_string());
    let destroy = host.destroy_world(world).map_err(|e| e.to_string());
    let evidence = result?;
    unload?;
    destroy?;
    require(
        host.store_statistics().map_err(|e| e.to_string())?.0 == 0,
        "native worlds must drain",
    )?;
    let fresh = host.create_world().map_err(|e| e.to_string())?;
    let status = request(host, fresh, json!({"op":"input.status"}))?;
    let input = request(host, fresh, json!({"op":"input.get"}))?;
    host.destroy_world(fresh).map_err(|e| e.to_string())?;
    require(
        status["queued_events"] == 0 && input["keys"] == json!([]),
        "reload must retire prior domain queues and held keys",
    )?;
    let roots = host
        .observe(&json!({"op":"managed.statistics"}))
        .map_err(|e| e.to_string())?;
    for key in ["scripts", "coroutines", "bindings"] {
        require(roots["data"][key] == 0, "managed roots must drain")?;
    }
    Ok(evidence)
}
fn run(host: &mut CompatibilityHost, binding: &Binding, world: u64) -> Result<Value, String> {
    configure(host, binding, world)?;
    host.open_phase_session(world).map_err(|e| e.to_string())?;
    let (manifest, platform) = fixture(host, binding, world)?;
    host.load_fixture(&manifest).map_err(|e| e.to_string())?;
    let loaded = sample(host, "LoadedImages")?;
    binding.verify_managed(&loaded)?;
    let short = short_tap(host, world)?;
    let (held, position) = held_and_precedence(host, world, &platform)?;
    let focus = focus_loss(host, world)?;
    let negative = negative_axes(host, world)?;
    let independent = independent_modes(host, world)?;
    events(host, world, json!([key(12, 200, "D", true)]))?;
    Ok(
        json!({"loaded_managed_images":loaded,"short_tap":short,"held":held,"focus_loss":focus,"negative_axes":negative,"independent_modes":independent,"platform_position":position,"queued_before_unload":request(host,world,json!({"op":"input.status"}))?}),
    )
}
fn short_tap(host: &mut CompatibilityHost, world: u64) -> Result<(Value, Value), String> {
    events(
        host,
        world,
        json!([key(1, 1, "W", true), key(2, 2, "W", false)]),
    )?;
    let short = frame(host, world)?;
    for row in [&short.0, &short.1] {
        require(
            row["held"][0] == false
                && row["down"][0] == true
                && row["up"][0] == true
                && row["repeated"] == true,
            "short tap and repeated polls survive both native projections",
        )?;
    }
    Ok(short)
}
fn held_and_precedence(
    host: &mut CompatibilityHost,
    world: u64,
    platform: &Value,
) -> Result<((Value, Value), Value), String> {
    events(
        host,
        world,
        json!([
            key(3, 21, "W", true),
            key(4, 21, "A", true),
            key(5, 21, "D", true),
            key(6, 21, "UpArrow", true)
        ]),
    )?;
    let held = frame(host, world)?;
    for row in [&held.0, &held.1] {
        require(
            row["held"] == json!([true, true, false, true])
                && row["vertical"] == 1.
                && row["alternate"] == 1.,
            "WASD and imported Pong axes",
        )?;
    }
    let position = request(host, world, json!({"op":"transform.get","object":platform}))?;
    require(
        position["position"][0]
            .as_f64()
            .is_some_and(|x| x > 0.19 && x < 0.21),
        "unchanged Platform gives D precedence over A",
    )?;
    Ok((held, position))
}
fn focus_loss(host: &mut CompatibilityHost, world: u64) -> Result<(Value, Value), String> {
    events(
        host,
        world,
        json!([{"event_id":7,"timestamp":{"num":41,"den":1000},"source":"os","kind":"focus","focused":false}]),
    )?;
    let focus = frame(host, world)?;
    for row in [&focus.0, &focus.1] {
        require(
            row["held"] == json!([false, false, false, false])
                && row["up"][0] == true
                && row["vertical"] == 0.,
            "focus loss releases held input",
        )?;
    }
    Ok(focus)
}
fn negative_axes(host: &mut CompatibilityHost, world: u64) -> Result<(Value, Value), String> {
    events(
        host,
        world,
        json!([{"event_id":8,"timestamp":{"num":61,"den":1000},"source":"os","kind":"focus","focused":true},key(9,62,"S",true),key(10,62,"DownArrow",true)]),
    )?;
    let negative = frame(host, world)?;
    require(
        negative.0["vertical"] == -1. && negative.1["alternate"] == -1.,
        "negative imported Pong bindings",
    )?;
    Ok(negative)
}
fn independent_modes(host: &mut CompatibilityHost, world: u64) -> Result<Value, String> {
    events(host, world, json!([key(11, 81, "D", true)]))?;
    host.advance_rational(world, 1, 100)
        .map_err(|e| e.to_string())?;
    let dynamic = sample(host, "DynamicSample")?;
    let fixed = sample(host, "FixedSample")?;
    require(
        dynamic["held"][3] == true && fixed["held"][3] == false,
        "dynamic input projection must not advance the fixed callback",
    )?;
    host.advance_rational(world, 1, 100)
        .map_err(|e| e.to_string())?;
    let next = sample(host, "FixedSample")?;
    require(
        next["held"][3] == true && next["down"][3] == true,
        "fixed projection retains input until its own cutoff",
    )?;
    Ok(json!({"dynamic":dynamic,"prior_fixed":fixed,"next_fixed":next}))
}

fn configure(host: &CompatibilityHost, binding: &Binding, world: u64) -> Result<(), String> {
    let request = compat_kit::RunRequest {
        schema_version: 1,
        run_id: "provider-qualification".into(),
        fixture: "pong".into(),
        profile: "keyboard-unlit-v1".into(),
        desktop: false,
        seed: 1,
        timeline: vec![],
        assertions: vec![],
        source_root: binding.path(".work/games/pong"),
        source_hashes: std::collections::BTreeMap::default(),
        artifact_root: binding.path(".work/resumed-provider-qualification-v2"),
        metadata: json!({}),
    };
    for command in crate::resources::commands(binding.root(), &request)? {
        self::request(host, world, command)?;
    }
    Ok(())
}
fn object(host: &CompatibilityHost, world: u64, name: &str) -> Result<Value, String> {
    Ok(request(host, world, json!({"op":"object.create","name":name}))?["handle"].clone())
}
fn component(
    host: &CompatibilityHost,
    world: u64,
    object: &Value,
    kind: &str,
    fields: impl Into<Value>,
) -> Result<Value, String> {
    let fields = fields.into();
    Ok(request(
        host,
        world,
        json!({"op":"component.add","object":object,"kind":kind,"fields":fields}),
    )?["handle"]
        .clone())
}
fn fixture(
    host: &CompatibilityHost,
    binding: &Binding,
    world: u64,
) -> Result<(Value, Value), String> {
    let probe = object(host, world, "provider-probe")?;
    let probe_script = component(
        host,
        world,
        &probe,
        "Script",
        json!({"type":"ProviderKeyboardProbe"}),
    )?;
    let platform = object(host, world, "original-platform")?;
    let platform_script = component(host, world, &platform, "Script", json!({"type":"Platform"}))?;
    component(
        host,
        world,
        &platform,
        "Rigidbody2D",
        json!({"body_type":"kinematic","gravity_scale":0}),
    )?;
    component(host, world, &platform, "BoxCollider2D", json!({}))?;
    component(host, world, &platform, "SpriteRenderer", json!({}))?;
    let manifest = json!({"world":world,"requirements":{"execution_profile":"native_phase_context_v1"},"assemblies":[binding.probe(),binding.path(".work/resumed-provider-qualification-v2/managed/artifacts/bin/Breakout/debug/Breakout.dll")],"handles":{"probe":probe,"probe-script":probe_script,"platform":platform,"platform-script":platform_script},"graph":{"nodes":[{"id":"probe","template":false,"active":true},{"id":"platform","template":false,"active":true}],"components":[{"id":"probe-script","object":"probe","script":{"type_name":"ProviderKeyboardProbe","fields":{}}},{"id":"platform-script","object":"platform","script":{"type_name":"Platform","fields":{}}}],"assets":[],"aliases":{}}});
    Ok((manifest, platform))
}
