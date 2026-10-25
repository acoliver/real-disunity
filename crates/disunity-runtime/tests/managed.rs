use std::path::{Path, PathBuf};
use std::process::Command;
use worldrunner_disunity_runtime::{DisunityHost, HostError, ManagedHostConfig};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(command: &mut Command) {
    let output = command
        .output()
        .expect("verification tool must be installed");
    assert!(
        output.status.success(),
        "{command:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn missing_explicit_artifacts_fail_without_claiming_the_runtime() {
    let config = ManagedHostConfig {
        ffi_library: workspace().join("missing-engine-library"),
        runtime_config: workspace().join("missing-runtime-config"),
        assembly: workspace().join("missing-managed-assembly"),
        bridge_type: "PortableFixture.NativeBridge, PortableFixture".into(),
    };
    for _ in 0..2 {
        assert!(matches!(
            DisunityHost::boot(&config),
            Err(HostError::ManagedArtifactsMissing(_))
        ));
    }
}

fn build_fixture(root: &Path) -> ManagedHostConfig {
    run(
        Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args([
                "build",
                "--locked",
                "--jobs",
                "2",
                "-p",
                "worldrunner-disunity-ffi",
            ])
            .current_dir(root),
    );
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let name = format!(
        "{}worldrunner_disunity_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    run(Command::new(&dotnet)
        .args(["build", "--warnaserror"])
        .arg(root.join("csharp/UnityConsumer/UnityConsumer.csproj")));
    run(Command::new(&dotnet)
        .args(["run", "--project"])
        .arg(root.join("csharp/Disunity.Tests/Disunity.Tests.csproj"))
        .arg("-p:TreatWarningsAsErrors=true")
        .arg("--")
        .arg(target.join("debug").join(&name)));
    run(Command::new(&dotnet)
        .args(["build", "--warnaserror"])
        .arg(root.join("csharp/PortableFixture/PortableFixture.csproj")));
    let managed = root.join("csharp/PortableFixture/bin/Debug/net8.0");
    ManagedHostConfig {
        ffi_library: target.join("debug").join(name),
        runtime_config: managed.join("PortableFixture.runtimeconfig.json"),
        assembly: managed.join("PortableFixture.dll"),
        bridge_type: "PortableFixture.NativeBridge, PortableFixture".into(),
    }
}

fn assert_triangle(host: &DisunityHost) {
    let mut states = host.export_objects().unwrap();
    states.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(states.len(), 3);
    for (state, expected) in states.iter().zip([
        (0.0, 0.0, 0.0, 0),
        (2.0, -3.0, 0.25, 1),
        (4.0, -6.0, 0.5, 2),
    ]) {
        assert!((state.x - expected.0).abs() < f32::EPSILON);
        assert!((state.y - 1.25).abs() < f32::EPSILON);
        assert!((state.z - expected.1).abs() < f32::EPSILON);
        assert!((state.rotation_y_radians - expected.2).abs() < f32::EPSILON);
        assert_eq!(state.material_slot, expected.3);
    }
}

fn assert_lifecycle_soak(host: &mut DisunityHost, fixture: &Path) {
    let mut first_slots = None;
    for cycle in 0..100 {
        host.run_slice(fixture).expect("synthetic fixture loads");
        assert_eq!(host.token_live(), 1, "cycle {cycle}");
        assert!(host.engine_token_live());
        assert_eq!(host.store_statistics().0, 1, "cycle {cycle}");
        assert!(matches!(
            host.run_slice(fixture),
            Err(HostError::WorldAlreadyLoaded)
        ));
        assert_triangle(host);
        host.unload_slice().unwrap();
        assert_eq!(host.token_live(), 0, "cycle {cycle}");
        assert!(!host.engine_token_live());
        let (live, slots) = host.store_statistics();
        assert_eq!(live, 0, "cycle {cycle}");
        if let Some(expected) = first_slots {
            assert_eq!(slots, expected, "cycle {cycle}");
        } else {
            first_slots = Some(slots);
        }
    }
    println!(
        "PASS: 100 real C# load/step/export/unload cycles; native worlds and managed tokens drained"
    );
}

fn assert_failed_boots_release_the_claim(root: &Path, config: &ManagedHostConfig) {
    let mut invalid = config.clone();
    invalid.ffi_library = root.join("missing-engine-library");
    for _ in 0..2 {
        assert!(matches!(
            DisunityHost::boot(&invalid),
            Err(HostError::FfiNotFound(_))
        ));
    }
    invalid = config.clone();
    invalid.bridge_type = "PortableFixture.MissingBridge, PortableFixture".into();
    assert!(matches!(
        DisunityHost::boot(&invalid),
        Err(HostError::Hostfxr(_))
    ));
}

#[test]
fn explicit_bridge_exports_transforms_and_drains_for_one_hundred_cycles() {
    let root = workspace();
    let config = build_fixture(&root);
    assert_failed_boots_release_the_claim(&root, &config);
    let mut host = DisunityHost::boot(&config).expect("explicit host boots after failed boots");
    assert!(matches!(
        DisunityHost::boot(&config),
        Err(HostError::AlreadyBooted)
    ));
    assert!(matches!(host.export_objects(), Err(HostError::NoWorld)));
    let fixture = root.join("csharp/PortableFixture/triangle.fixture");
    assert!(host.run_slice(&root.join("missing.fixture")).is_err());
    assert_eq!(host.token_live(), 0);
    assert!(!host.engine_token_live());
    assert_eq!(host.store_statistics().0, 0);
    assert_failed_runs_drain(&mut host, &root);
    assert_lifecycle_soak(&mut host, &fixture);
    host.run_slice(&fixture).unwrap();
    assert_cross_language_writes(&mut host);
    drop(host);
    let host = DisunityHost::boot(&config).unwrap();
    assert_eq!(host.store_statistics().0, 0, "drop unloads an active world");
    assert_eq!(host.token_live(), 0, "drop frees the managed token");
}

fn assert_failed_runs_drain(host: &mut DisunityHost, root: &Path) {
    for _ in 0..2 {
        assert!(
            host.run_slice(&root.join("csharp/PortableFixture/load-failure.fixture"))
                .is_err()
        );
        assert_eq!(host.store_statistics().0, 0);
        assert_eq!(host.token_live(), 0);
        assert!(!host.engine_token_live());
    }
    assert_callback_failure(host, root, "callback-failure.fixture");
    assert_callback_failure(host, root, "long-failure.fixture");
}

fn assert_callback_failure(host: &mut DisunityHost, root: &Path, fixture: &str) {
    host.run_slice(&root.join("csharp/PortableFixture").join(fixture))
        .unwrap();
    let failed = host.advance(1, 100).unwrap_err();
    assert!(
        matches!(&failed, HostError::Managed(message) if message.contains("managed callback failure λ")),
        "{failed:?}"
    );
    assert!(
        host.export_objects().unwrap().is_empty(),
        "failure teardown retires ABI object bindings"
    );
    let faulted = host.advance(0, 1).unwrap_err();
    assert!(matches!(
        faulted,
        HostError::Native {
            reason: worldrunner_disunity_runtime::DisunityErrorReason::Faulted,
            ..
        }
    ));
    host.unload_slice().unwrap();
    assert_eq!(host.store_statistics().0, 0);
    assert_eq!(host.token_live(), 0);
}

fn assert_cross_language_writes(host: &mut DisunityHost) {
    let object = host.export_objects().unwrap()[0].object;
    host.set_position(object, [17.0, 23.0, 31.0]).unwrap();
    assert_eq!(host.managed_position(object).unwrap(), [17.0, 23.0, 31.0]);
    host.advance(1, 100).unwrap();
    assert_eq!(host.managed_position(object).unwrap(), [17.0, 23.0, 31.0]);
    let native = host
        .export_objects()
        .unwrap()
        .into_iter()
        .find(|row| row.object == object)
        .unwrap();
    assert_eq!([native.x, native.y, native.z], [17.0, 23.0, 31.0]);
}
