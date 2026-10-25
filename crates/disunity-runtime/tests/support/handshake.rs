#[path = "managed.rs"]
pub(crate) mod managed;

use std::{path::PathBuf, process::Command, sync::OnceLock};
use worldrunner_disunity_runtime::ManagedHostConfig;

#[path = "child.rs"]
pub mod child;

pub struct Artifacts {
    pub native: PathBuf,
    pub managed_root: PathBuf,
    _build_lock: Option<std::fs::File>,
}

impl Artifacts {
    pub fn config(&self) -> ManagedHostConfig {
        ManagedHostConfig {
            ffi_library: self.native.clone(),
            runtime_config: self.bridge().with_extension("runtimeconfig.json"),
            assembly: self.bridge(),
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        }
    }

    pub fn bridge(&self) -> PathBuf {
        self.managed_root
            .join("artifacts/bin/CompatibilityBridge/debug/CompatibilityBridge.dll")
    }

    pub fn probe(&self) -> PathBuf {
        self.managed_root
            .join("artifacts/bin/Compatibility.Tests/debug/Compatibility.Tests.dll")
    }
}

pub fn artifacts() -> &'static Artifacts {
    static ARTIFACTS: OnceLock<Artifacts> = OnceLock::new();
    ARTIFACTS.get_or_init(build)
}

fn build() -> Artifacts {
    let root = worldrunner_original_games::workspace_root().expect("workspace must exist");
    let outputs = root.join("target/test-artifacts");
    std::fs::create_dir_all(&outputs).unwrap();
    if child::active() {
        return Artifacts {
            native: native_path(&outputs),
            managed_root: outputs.join("managed"),
            _build_lock: None,
        };
    }
    let build_lock = std::fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(outputs.join("build.lock"))
        .unwrap();
    // Keep loaded assemblies and native images immutable until this test process exits.
    build_lock
        .lock()
        .expect("test artifact lock must be available");
    let managed_root = outputs.join("managed");
    let native = build_native(&root, &outputs);
    let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    managed::build_suites(&root, &dotnet, &managed_root.join("artifacts"));
    let result = Artifacts {
        native,
        managed_root,
        _build_lock: Some(build_lock),
    };
    assert!(result.config().assembly.is_file() && result.probe().is_file());
    result
}

fn build_native(root: &std::path::Path, outputs: &std::path::Path) -> PathBuf {
    let target = outputs.join("native");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    managed::execute(
        Command::new(cargo)
            .current_dir(root)
            .env("CARGO_TARGET_DIR", &target)
            .args([
                "build",
                "--locked",
                "--offline",
                "--jobs",
                "2",
                "-p",
                "worldrunner-disunity-ffi",
            ]),
    );
    native_path(outputs)
}

fn native_path(outputs: &std::path::Path) -> PathBuf {
    outputs.join("native/debug").join(format!(
        "{}worldrunner_disunity_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX,
    ))
}
