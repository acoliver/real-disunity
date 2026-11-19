use super::*;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn dotnet(arguments: &[&str], root: &Path) {
    let executable = std::env::var_os("DOTNET_HOST_PATH").unwrap_or_else(|| "dotnet".into());
    let output = Command::new(executable)
        .args(arguments)
        .current_dir(root)
        .env("DOTNET_CLI_DO_NOT_USE_MSBUILD_SERVER", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn managed_owned_reads_use_the_real_native_worker() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    dotnet(
        &[
            "restore",
            "csharp/Disunity.Tests/Disunity.Tests.csproj",
            "--locked-mode",
            "-p:NuGetAudit=false",
            "-m:1",
            "-nr:false",
        ],
        &root,
    );
    dotnet(
        &[
            "build",
            "csharp/Disunity.Tests/Disunity.Tests.csproj",
            "-c",
            "Release",
            "--no-restore",
            "--warnaserror",
            "-m:1",
            "-nr:false",
            "-p:UseSharedCompilation=false",
            "-p:BuildInParallel=false",
        ],
        &root,
    );
    let directory = root.join("csharp/Disunity.Tests/bin/Release/net8.0");
    let config = ManagedHostConfig {
        assembly: directory.join("Disunity.Tests.dll"),
        runtime_config: directory.join("Disunity.Tests.runtimeconfig.json"),
        bridge_type: "AsyncReadOwnedTests, Disunity.Tests".into(),
        ffi_library: PathBuf::new(),
    };
    let loader = crate::managed_entrypoints::managed_loader(&config).unwrap();
    let name = |text: &str| PdCString::from_os_str(OsStr::new(text)).unwrap();
    let test_type = name("AsyncReadOwnedTests, Disunity.Tests");
    let run = *loader.get_function_with_unmanaged_callers_only::<unsafe extern "system" fn(*const std::ffi::c_char) -> i32>(
        &test_type, &name("RunNative"),
    ).unwrap();
    let after_detach = *loader
        .get_function_with_unmanaged_callers_only::<unsafe extern "system" fn() -> i32>(
            &test_type,
            &name("AfterDetach"),
        )
        .unwrap();
    let fixture = crate::file_reads_tests::Fixture::new();
    let path = CString::new(fixture.0.to_str().unwrap()).unwrap();
    let bridge = FileReadBridge::attach(&config).unwrap();
    // SAFETY: the built test entry points accept exactly these signatures and borrow the fixture path.
    assert_eq!(unsafe { run(path.as_ptr()) }, 0);
    drop(bridge);
    // SAFETY: the same test assembly remains mapped in CoreCLR after callback detachment.
    assert_eq!(unsafe { after_detach() }, 0);
    let bridge = FileReadBridge::attach(&config).unwrap();
    // SAFETY: verifies a fresh host session can submit after the old one ended.
    assert_eq!(unsafe { run(path.as_ptr()) }, 0);
    drop(bridge);
    assert_eq!(unsafe { after_detach() }, 0);
}

#[test]
fn callback_rejects_invalid_foreign_inputs_without_writes() {
    let mut reader = Mutex::new(FileReads::new(ReadLimits::default()).unwrap());
    let context = (&mut reader as *mut Mutex<FileReads>).cast();
    let mut bytes = [0xa5; 8];
    // SAFETY: each call borrows this local reader and the declared writable array, or passes an explicitly rejected null.
    unsafe {
        assert_eq!(
            call(std::ptr::null_mut(), 1, 0, std::ptr::null_mut(), 0, 0),
            -2
        );
        assert_eq!(call(context, 0, 0, std::ptr::null_mut(), 1, 1), -2);
        assert_eq!(call(context, 0, 0, bytes.as_mut_ptr(), 8, 1), -2);
        assert_eq!(call(context, 1, 0, std::ptr::null_mut(), 0, 0), -1);
        assert_eq!(call(context, 6, 0, std::ptr::null_mut(), 0, 0), -1);
    }
    assert_eq!(bytes, [0xa5; 8]);
}
