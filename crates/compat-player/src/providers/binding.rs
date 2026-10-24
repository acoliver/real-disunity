#[cfg(test)]
#[path = "binding_tests.rs"]
mod tests;

use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use worldrunner::{
    compat_content::{CapabilityId, CapabilityRequirement, ResourceKind, TrustedProvider},
    managed::{CompatibilityHost, ManagedHostConfig},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Seal {
    files: BTreeMap<String, String>,
    native: String,
    bridge: String,
    probe: String,
    contract: String,
}

pub(crate) struct Binding {
    root: PathBuf,
    seal: Seal,
    implementation: Vec<u8>,
}
impl Binding {
    pub fn load(root: &Path) -> Result<Self, String> {
        let seal: Seal = serde_json::from_str(include_str!("qualified-build.json"))
            .map_err(|e| format!("unqualified provider build: {e}"))?;
        if seal.contract != "keyboard-unlit-v1" || seal.files.is_empty() {
            return Err("provider has no retained qualification".into());
        }
        let mut binding = Self {
            root: root.canonicalize().map_err(|e| e.to_string())?,
            seal,
            implementation: Vec::new(),
        };
        binding.verify()?;
        for &(relative, compiled) in super::compiled_inputs::inputs() {
            if !binding.seal.files.contains_key(relative) {
                return Err(format!("compiled provider input is unsealed: {relative}"));
            }
            let selected = std::fs::read(binding.path(relative)).map_err(|e| e.to_string())?;
            if selected != compiled {
                return Err(format!("compiled provider input differs: {relative}"));
            }
        }
        for path in [&binding.seal.native, &binding.seal.bridge] {
            binding
                .implementation
                .extend(std::fs::read(binding.path(path)).map_err(|e| e.to_string())?);
        }
        binding
            .implementation
            .extend_from_slice(include_bytes!("qualified-build.json"));
        Ok(binding)
    }
    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn probe(&self) -> PathBuf {
        self.path(&self.seal.probe)
    }
    pub fn config(&self) -> ManagedHostConfig {
        let assembly = self.path(&self.seal.bridge);
        ManagedHostConfig {
            ffi_library: self.path(&self.seal.native),
            runtime_config: assembly.with_extension("runtimeconfig.json"),
            assembly,
            bridge_type: "CompatibilityBridge.ManagedBridge, CompatibilityBridge".into(),
        }
    }
    pub fn verify(&self) -> Result<(), String> {
        for (relative, expected) in &self.seal.files {
            compat_kit::files::relative_path(Path::new(relative))?;
            let path = compat_kit::files::safe_file(&self.root, Path::new(relative))?;
            if compat_kit::files::hash(&path)? != *expected {
                return Err(format!("qualified input changed: {relative}"));
            }
        }
        Ok(())
    }
    pub fn verify_loaded_native(&self) -> Result<(), String> {
        let maps = std::fs::read_to_string("/proc/self/maps").map_err(|e| e.to_string())?;
        let path = self.path(&self.seal.native);
        if !maps
            .lines()
            .any(|line| line.split_whitespace().last() == path.to_str())
        {
            return Err("qualified native image is not actually mapped".into());
        }
        Ok(())
    }
    pub fn verify_managed(&self, sample: &Value) -> Result<(), String> {
        let rows = sample.as_array().ok_or("missing loaded managed identity")?;
        for path in [
            self.seal.probe.clone(),
            self.seal.bridge.clone(),
            self.seal
                .bridge
                .replace("CompatibilityBridge.dll", "UnityCompat.dll"),
        ] {
            let expected = compat_kit::files::hash(&self.path(&path))?;
            if !rows.iter().any(|row| {
                row["path"].as_str() == self.path(&path).to_str() && row["sha256"] == expected
            }) {
                return Err(format!("actual managed image differs: {path}"));
            }
        }
        Ok(())
    }
}

pub struct QualifiedProviders {
    host: CompatibilityHost,
    binding: Binding,
    providers: [Provider; 2],
}
impl QualifiedProviders {
    pub(super) fn new(
        host: CompatibilityHost,
        binding: Binding,
        input: &Value,
        render: &Value,
    ) -> Self {
        let evidence = serde_json::to_vec(&json!({"contract":"keyboard-unlit-v1", "tuple":binding.seal.files,"input":input,"render":render})).expect("bounded JSON evidence serializes");
        let providers = [
            Provider::new(
                ResourceKind::Input,
                CapabilityId::CanonicalKeyboardSnapshot,
                &binding.implementation,
                &evidence,
            ),
            Provider::new(
                ResourceKind::RenderSettings,
                CapabilityId::CanonicalUnlitQuads,
                &binding.implementation,
                &evidence,
            ),
        ];
        Self {
            host,
            binding,
            providers,
        }
    }
    pub(super) fn binding_for_source(&self) -> &Binding {
        &self.binding
    }
    pub fn host_mut(&mut self) -> &mut CompatibilityHost {
        &mut self.host
    }
    pub fn evidence_bytes(&self) -> &[u8] {
        &self.providers[0].evidence
    }
    pub(super) fn verify_current(&self) -> Result<(), String> {
        self.binding.verify()?;
        self.binding.verify_loaded_native()?;
        self.host
            .request(0, &json!({"op":"describe"}))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(super) fn providers(&self) -> [&dyn TrustedProvider; 2] {
        [&self.providers[0], &self.providers[1]]
    }
}
struct Provider {
    kind: ResourceKind,
    capability: CapabilityId,
    implementation: Vec<u8>,
    evidence: Vec<u8>,
}
impl Provider {
    fn new(
        kind: ResourceKind,
        capability: CapabilityId,
        implementation: &[u8],
        evidence: &[u8],
    ) -> Self {
        Self {
            kind,
            capability,
            implementation: implementation.to_vec(),
            evidence: evidence.to_vec(),
        }
    }
}
impl TrustedProvider for Provider {
    fn id(&self) -> &str {
        match self.kind {
            ResourceKind::Input => "host.keyboard-snapshot.v1",
            _ => "host.canonical-unlit.v1",
        }
    }
    fn resource_kind(&self) -> ResourceKind {
        self.kind
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        vec![CapabilityRequirement {
            id: self.capability,
            contract_version: 1,
            resource_kind: self.kind,
        }]
    }
    fn implementation_bytes(&self) -> &[u8] {
        &self.implementation
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        &self.evidence
    }
}
