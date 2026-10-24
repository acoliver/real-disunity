use crate::{ContentReceipt, ScriptSource};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ApiSurface {
    KeyboardSnapshot,
    CanonicalFacade,
    InputActions,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyApiUse {
    pub assembly: String,
    pub type_name: String,
    pub member: String,
    pub surface: ApiSurface,
}
/// The host verifies the actual `GameSource` PE and its complete transitive API-use
/// closure before implementing this boundary. JSON is never a qualification.
pub trait TrustedGameSourceProof {
    fn profile_id(&self) -> &str;
    fn content_closure_sha256(&self) -> &str;
    fn source_directory(&self) -> &Path;
    fn sources(&self) -> Vec<ScriptSource>;
    fn assembly_bytes(&self) -> &[u8];
    fn verification_evidence_bytes(&self) -> &[u8];
    fn defined_types(&self) -> BTreeMap<String, String>;
    fn api_uses(&self) -> Vec<AssemblyApiUse>;
}
#[derive(Clone, Debug, Serialize)]
pub struct GameSourceQualification {
    profile_id: String,
    closure: String,
    directory: std::path::PathBuf,
    sources: BTreeMap<String, String>,
    assembly_sha256: String,
    evidence_sha256: String,
    types: BTreeMap<String, String>,
    api_uses: Vec<AssemblyApiUse>,
}
impl GameSourceQualification {
    pub fn from_host(host: &dyn TrustedGameSourceProof) -> Result<Self, String> {
        let bytes = host.assembly_bytes();
        let pe = bytes
            .get(60..64)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes);
        if bytes.get(..2) != Some(b"MZ")
            || pe.and_then(|p| bytes.get(p as usize..p as usize + 4)) != Some(b"PE\0\0")
            || host.verification_evidence_bytes().is_empty()
        {
            return Err("host proof requires actual PE and verification evidence bytes".into());
        }
        let entries = host.sources();
        let sources: BTreeMap<_, _> = entries
            .iter()
            .map(|s| (s.path.clone(), s.sha256.clone()))
            .collect();
        if sources.len() != entries.len() {
            return Err("duplicate source proof entries".into());
        }
        Ok(Self {
            profile_id: host.profile_id().into(),
            closure: host.content_closure_sha256().into(),
            directory: host.source_directory().into(),
            sources,
            assembly_sha256: crate::dependency::hash(bytes),
            evidence_sha256: crate::dependency::hash(host.verification_evidence_bytes()),
            types: host.defined_types(),
            api_uses: host.api_uses(),
        })
    }
    pub fn sha256(&self) -> String {
        crate::dependency::identity(self)
    }
    pub(super) fn has_type(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }
    pub(super) fn validate(
        &self,
        receipt: &ContentReceipt,
        profile: &str,
        keyboard: bool,
    ) -> Result<(), String> {
        if self.profile_id != profile || self.closure != receipt.closure_sha256() {
            return Err("source qualification profile/content closure mismatch".into());
        }
        for (path, hash) in &self.sources {
            if receipt
                .sources()
                .get(path)
                .is_some_and(|b| crate::dependency::hash(b) != *hash)
            {
                return Err("source qualification bytes mismatch".into());
            }
        }
        if self.types.values().any(|p| !self.sources.contains_key(p)) {
            return Err("defined type has no selected source".into());
        }
        for script in receipt
            .graph()
            .components
            .iter()
            .filter_map(|c| c.script.as_ref())
        {
            if self.types.get(&script.type_name) != Some(&script.path) {
                return Err(format!(
                    "GameSource PE does not define selected type {}",
                    script.type_name
                ));
            }
        }
        if keyboard
            && (self.sources.is_empty() || self.types.is_empty() || self.api_uses.is_empty())
        {
            return Err(
                "keyboard selection requires complete typed GameSource PE/API-use proof".into(),
            );
        }
        if self
            .api_uses
            .iter()
            .any(|api| !super::api::keyboard_use(api))
        {
            return Err("selected assembly uses unqualified action/wrapper or unknown API".into());
        }
        if self
            .api_uses
            .iter()
            .any(|a| a.assembly.is_empty() || a.type_name.is_empty() || a.member.is_empty())
        {
            return Err("incomplete typed assembly API-use entry".into());
        }
        Ok(())
    }
    pub(super) fn verify_current(&self, root: &Path) -> Result<(), String> {
        let current = crate::script_census(root, &self.directory, &crate::ImportProfile::default())
            .map_err(|e| e.to_string())?;
        let sources: BTreeMap<_, _> = current.into_iter().map(|s| (s.path, s.sha256)).collect();
        if sources != self.sources {
            return Err("complete GameSource census changed".into());
        }
        Ok(())
    }
}
