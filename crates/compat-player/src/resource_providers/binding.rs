use serde::Deserialize;
use std::os::unix::fs::MetadataExt;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use worldrunner::managed::ManagedHostConfig;

#[path = "inputs.rs"]
mod inputs;
const BASE: &str = ".work/resumed-resource-providers-exact-v6";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tuple {
    files: BTreeMap<String, String>,
    scope: String,
}
pub(super) struct Binding {
    root: PathBuf,
    tuple: Tuple,
    implementation: Vec<u8>,
    executable: PathBuf,
    image_sha256: String,
}
impl Binding {
    pub fn load(root: &Path) -> Result<Self, String> {
        Self::load_with_inputs(root, inputs::all().copied())
    }
    /// Byte witnesses compiled into this image, for stale-witness rejection tests.
    #[cfg(test)]
    pub(super) fn compiled_inputs() -> impl Iterator<Item = (&'static str, &'static [u8])> {
        inputs::all().copied()
    }
    /// Verifies the tuple, then compares each selected file with its compiled witness.
    /// `load` passes this image's own witnesses; tests pass a deliberately stale table.
    pub(super) fn load_with_inputs<'a>(
        root: &Path,
        compiled: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    ) -> Result<Self, String> {
        let tuple: Tuple =
            serde_json::from_slice(include_bytes!("tuple.json")).map_err(|e| e.to_string())?;
        if tuple.scope != "resource-canonical-exact-v6" || tuple.files.is_empty() {
            return Err("unqualified resource tuple".into());
        }
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let image_sha256 = compat_kit::files::hash(Path::new("/proc/self/exe"))?;
        let mut binding = Self {
            root,
            tuple,
            implementation: Vec::new(),
            executable,
            image_sha256,
        };
        binding.verify()?;
        for (relative, compiled) in compiled {
            let path = binding.path(relative);
            if std::fs::read(path).map_err(|e| e.to_string())? != compiled {
                return Err(format!("compiled resource input differs: {relative}"));
            }
        }
        for relative in binding.tuple.files.keys() {
            let bytes = std::fs::read(binding.path(relative)).map_err(|e| e.to_string())?;
            binding
                .implementation
                .extend_from_slice(relative.as_bytes());
            binding.implementation.push(0);
            binding
                .implementation
                .extend_from_slice(&bytes.len().to_le_bytes());
            binding.implementation.extend(bytes);
        }
        binding
            .implementation
            .extend_from_slice(binding.image_sha256.as_bytes());
        binding
            .implementation
            .extend_from_slice(include_bytes!("tuple.json"));
        Ok(binding)
    }
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn implementation(&self) -> &[u8] {
        &self.implementation
    }
    pub fn image_sha256(&self) -> &str {
        &self.image_sha256
    }
    pub fn files(&self) -> &BTreeMap<String, String> {
        &self.tuple.files
    }
    pub fn config(&self) -> ManagedHostConfig {
        let assembly = self.path(&format!(
            "{BASE}/managed/artifacts/bin/CompatibilityBridge/debug/CompatibilityBridge.dll"
        ));
        ManagedHostConfig {
            ffi_library: self.path(&format!("{BASE}/native.so")),
            runtime_config: assembly.with_extension("runtimeconfig.json"),
            assembly,
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        }
    }
    pub fn fixture_assembly(&self) -> PathBuf {
        self.path(&format!(
            "{BASE}/managed/artifacts/bin/Compatibility.Tests/debug/Compatibility.Tests.dll"
        ))
    }
    pub fn verify(&self) -> Result<(), String> {
        for (relative, expected) in &self.tuple.files {
            let path = compat_kit::files::safe_file(&self.root, Path::new(relative))?;
            if compat_kit::files::hash(&path)? != *expected {
                return Err(format!("resource qualification input changed: {relative}"));
            }
        }
        let selected = std::fs::metadata(&self.executable).map_err(|e| e.to_string())?;
        let mapped = std::fs::metadata("/proc/self/exe").map_err(|e| e.to_string())?;
        if selected.dev() != mapped.dev()
            || selected.ino() != mapped.ino()
            || compat_kit::files::hash(Path::new("/proc/self/exe"))? != self.image_sha256
        {
            return Err("resource qualification process image changed".into());
        }
        Ok(())
    }
    pub fn verify_mapped(&self) -> Result<(), String> {
        let maps = std::fs::read_to_string("/proc/self/maps").map_err(|e| e.to_string())?;
        let config = self.config();
        for image in [
            config.ffi_library,
            config.assembly.with_file_name("UnityCompat.dll"),
            config.assembly,
        ] {
            if !maps
                .lines()
                .any(|line| line.split_whitespace().last() == image.to_str())
            {
                return Err(format!(
                    "qualified resource image is not mapped: {}",
                    image.display()
                ));
            }
        }
        Ok(())
    }
}
