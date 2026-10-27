//! The sandbox closes the base library's ways out (#152): no file loaders, text chunks only, and
//! a memory limit the instruction budget cannot provide.

use worldrunner_script::{
    DEFAULT_MEMORY_LIMIT, HookContext, Overlay, ScriptConfig, ScriptError, ScriptHost,
};

fn host() -> ScriptHost {
    ScriptHost::new(ScriptConfig::default()).unwrap()
}

#[test]
fn scripts_cannot_reach_files() {
    let dir = std::env::temp_dir().join(format!("wr-sandbox-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("secret.lua");
    std::fs::write(&file, "return 'secret from disk'").unwrap();
    let path = file.to_str().unwrap().replace('\\', "/");
    let mut host = host();
    for loader in ["dofile", "loadfile"] {
        let reached = host
            .eval_json(&format!("return pcall({loader}, '{path}')"))
            .unwrap();
        assert_eq!(reached, serde_json::json!(false), "{loader} must not exist");
        assert_eq!(
            host.eval_json(&format!("return {loader} == nil")).unwrap(),
            serde_json::json!(true)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn load_compiles_text_and_refuses_bytecode_whatever_mode_is_asked() {
    let mut host = host();
    let results = host
        .eval_json(
            r#"
            local dumped = string.dump(function() return 42 end)
            local default, default_error = load(dumped)
            local binary, binary_error = load(dumped, "x", "b")
            return {
                load("return 1 + 1")(),
                load("return x", "n", "bt", { x = 5 })(),
                load("return worldrunner ~= nil")(),
                default == nil and default_error:find("binary") ~= nil,
                binary == nil and binary_error:find("binary") ~= nil,
            }
            "#,
        )
        .unwrap();
    assert_eq!(results, serde_json::json!([2, 5, true, true, true]));
}

#[test]
fn modules_must_be_source_text() {
    let bytecode = mlua::Lua::new()
        .load("return 42")
        .into_function()
        .unwrap()
        .dump(true);
    let mut host = host();
    let error = host
        .load_patched_module("compiled", &bytecode, &Overlay::default())
        .unwrap_err();
    assert!(error.to_string().contains("binary"), "{error}");
    host.load_module("text", "return 42").unwrap();
}

#[test]
fn allocation_is_bounded_by_the_memory_limit() {
    let mut host = host();
    let refused = host
        .eval_json(&format!(
            "return pcall(string.rep, 'x', {})",
            DEFAULT_MEMORY_LIMIT + 1
        ))
        .unwrap();
    assert_eq!(refused, serde_json::json!(false), "the default limit holds");
    let small = host.eval_json("return #string.rep('x', 1 << 20)").unwrap();
    assert_eq!(small, serde_json::json!(1 << 20));

    host.set_memory_limit(8 << 20).unwrap();
    host.load_module(
        "grow",
        "function on_update() big = string.rep('x', 16 << 20) end",
    )
    .unwrap();
    let error = host.fire("on_update", &HookContext::default()).unwrap_err();
    assert!(matches!(error, ScriptError::Memory { .. }), "{error}");
    assert!(matches!(
        host.set_memory_limit(0),
        Err(ScriptError::Sandbox { .. })
    ));
}
