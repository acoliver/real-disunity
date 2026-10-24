use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use worldrunner_disunity_runtime::{CompatibilityHost, ManagedHostConfig};

fn tool(command: &mut Command) {
    let output = command
        .output()
        .expect("integration prerequisite must be installed");
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn configure(root: &Path) -> (ManagedHostConfig, PathBuf) {
    let output = root.join(".work/runtime-host-probe");
    fs::create_dir_all(&output).unwrap();
    let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or("dotnet".into());
    tool(
        Command::new(&dotnet)
            .arg("build")
            .arg(root.join("csharp/CompatibilityBridge/CompatibilityBridge.csproj"))
            .arg("--artifacts-path")
            .arg(output.join("bridge"))
            .args(["--nologo", "-m:2"]),
    );
    let project = output.join("HostProbe.csproj");
    fs::write(&project,format!(r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework><EnableDefaultCompileItems>false</EnableDefaultCompileItems></PropertyGroup><ItemGroup><ProjectReference Include="{}/csharp/UnityCompat/UnityCompat.csproj"/><Compile Include="Probe.cs"/></ItemGroup></Project>"#,root.display())).unwrap();
    fs::write(output.join("Probe.cs"),r#"using UnityEngine;
public sealed class HostProbe : MonoBehaviour {
    [SerializeField] private int displacement;
    [SerializeField] private Transform target;
    private void Awake() { target.position = new Vector3(displacement, 0, 0); }
    private void Update() { if (Input.GetKeyDown(KeyCode.W)) target.position += new Vector3(Time.deltaTime, 1, 0); }
}"#).unwrap();
    tool(
        Command::new(&dotnet)
            .arg("build")
            .arg(project)
            .arg("--artifacts-path")
            .arg(output.join("game"))
            .args(["--nologo", "-m:2"]),
    );
    let bridge = output.join("bridge/bin/CompatibilityBridge/debug/CompatibilityBridge.dll");
    let target = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR").expect("source worktree environment before test"),
    );
    let ffi = target.join("debug").join(format!(
        "{}worldrunner_disunity_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    assert!(
        ffi.is_file(),
        "build the canonical FFI cdylib before this integration test"
    );
    (
        ManagedHostConfig {
            ffi_library: ffi,
            runtime_config: bridge.with_extension("runtimeconfig.json"),
            assembly: bridge,
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        },
        output.join("game/bin/HostProbe/debug/HostProbe.dll"),
    )
}

#[test]
fn live_private_callbacks_bind_before_awake_and_mutate_the_same_native_world() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let (config, assembly) = configure(&root);
    let mut host = CompatibilityHost::boot(&config).unwrap();
    assert_eq!(host.store_statistics().unwrap().0, 0);
    for _ in 0..2 {
        let (world, object) = load_probe(&mut host, &assembly);
        let awake = host
            .request(
                world,
                &json!({"version":1,"op":"transform.get","object":object}),
            )
            .unwrap();
        assert_eq!(awake["position"], json!([7., 0., 0.]));
        host.request(
            world,
            &json!({"version":1,"op":"input.set","keys":["W"],"axes":{}}),
        )
        .unwrap();
        host.request(world, &json!({"version":1,"op":"frame.begin","dt":0.25}))
            .unwrap();
        host.dispatch_phase(&json!({"phase":"Update"})).unwrap();
        let moved = host
            .request(
                world,
                &json!({"version":1,"op":"transform.get","object":object}),
            )
            .unwrap();
        assert_eq!(moved["position"], json!([7.25, 1., 0.]));
        let observed = host.observe(&json!({"op":"world.snapshot"})).unwrap();
        assert_eq!(
            observed["data"]["objects"][0]["transform"]["position"],
            moved["position"]
        );
        assert!(host.observe(&json!({"op":"unknown"})).is_err());
        host.unload().unwrap();
        let roots = host.observe(&json!({"op":"managed.statistics"})).unwrap();
        assert_eq!(roots["data"]["scripts"], 0);
        assert_eq!(roots["data"]["coroutines"], 0);
        host.destroy_world(world).unwrap();
        assert_eq!(host.store_statistics().unwrap().0, 0);
    }
}

fn load_probe(host: &mut CompatibilityHost, assembly: &Path) -> (u64, u64) {
    let world = host.create_world().unwrap();
    let request = |value: Value| host.request(world, &value).unwrap();
    let object = request(json!({"version":1,"op":"object.create","name":"live-probe"}))["handle"]
        .as_u64()
        .unwrap();
    let transform = request(
        json!({"version":1,"op":"component.list","object":object,"kind":"Transform"}),
    )[0]["handle"]
        .as_u64()
        .unwrap();
    let script = request(json!({"version":1,"op":"component.add","object":object,"kind":"Script","fields":{"type":"HostProbe"}}))["handle"].as_u64().unwrap();
    host.load_fixture(&json!({"world":world,"assemblies":[assembly],"handles":{"owner":object,"transform":transform,"script":script},"profile":{"id":"compat-6000.3"},"graph":{
            "nodes":[{"id":"owner","template":false,"active":true}],"assets":[],"aliases":{},
            "components":[
                {"id":"transform","object":"owner","script":null},
                {"id":"script","object":"owner","script":{"type_name":"HostProbe","fields":{"displacement":7,"target":{"$ref":"transform"}}}}
            ]}})).unwrap();
    (world, object)
}
