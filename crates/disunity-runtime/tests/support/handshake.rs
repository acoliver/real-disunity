#[path = "managed.rs"]
pub(crate) mod managed;

use std::{path::PathBuf, process::Command, sync::OnceLock};

pub struct Artifacts {
    pub native: PathBuf,
    pub managed_root: PathBuf,
    _build_lock: std::fs::File,
}

impl Artifacts {
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace must exist");
    let outputs = root.join(
        std::env::var_os("DISUNITY_TEST_OUTPUT_DIR")
            .unwrap_or_else(|| ".work/validation-handshake".into()),
    );
    std::fs::create_dir_all(&outputs).unwrap();
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
    let managed_root = if let Some(path) = std::env::var_os("DISUNITY_TEST_MANAGED_ROOT") {
        root.join(path)
    } else {
        let directory = outputs.join("managed");
        let dotnet = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
        managed::build_suites(&root, &dotnet, &directory.join("artifacts"));
        directory
    };
    let native = match std::env::var_os("DISUNITY_TEST_NATIVE_LIBRARY") {
        Some(path) => root.join(path),
        None => build_native(&root, &outputs),
    };
    Artifacts {
        native,
        managed_root,
        _build_lock: build_lock,
    }
}

fn build_native(root: &std::path::Path, outputs: &std::path::Path) -> PathBuf {
    let target = outputs.join("native-target");
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
    target.join("debug").join(format!(
        "{}worldrunner_disunity_ffi{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX,
    ))
}
