use super::{
    CapabilityRequirement, Deserialize, FieldSegment, OwnerIdentity, ReferenceLocator,
    ResourceKind, Serialize, SerializedReference,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Fidelity {
    Original,
    TranslatedUnlitKeyboardSubset,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyPolicy {
    pub schema_version: u32,
    pub profile_id: String,
    pub fidelity: Fidelity,
    pub required_capabilities: Vec<CapabilityRequirement>,
    pub rules: Vec<DependencyRule>,
}
impl Default for DependencyPolicy {
    fn default() -> Self {
        Self {
            schema_version: 1,
            profile_id: "strict-original".into(),
            fidelity: Fidelity::Original,
            required_capabilities: vec![],
            rules: vec![],
        }
    }
}
impl DependencyPolicy {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let policy: Self =
            serde_json::from_value(super::wire::parse(bytes)?).map_err(|e| e.to_string())?;
        policy.validate()?;
        Ok(policy)
    }
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.profile_id.trim().is_empty() {
            return Err("unsupported dependency policy version or empty profile".into());
        }
        let mut ids = BTreeSet::new();
        let mut locators = BTreeSet::new();
        for requirement in &self.required_capabilities {
            requirement.validate()?;
        }
        for rule in &self.rules {
            rule.validate()?;
            if !ids.insert(&rule.id) || !locators.insert(&rule.locator) {
                return Err("duplicate rule ID or locator".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyRule {
    pub id: String,
    pub locator: ReferenceLocator,
    pub source_sha256: String,
    pub expected_reference: SerializedReference,
    pub resource_kind: ResourceKind,
    pub disposition: DependencyDisposition,
    pub evidence: Vec<EvidenceRef>,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "classification", rename_all = "snake_case", deny_unknown_fields)]
pub enum DependencyDisposition {
    EditorConfiguration,
    ImportToolNotExecuted {
        capability: CapabilityRequirement,
    },
    RenderSettingsTranslated {
        capability: CapabilityRequirement,
    },
    ProvidedBuiltin {
        provider_id: String,
        resource_kind: ResourceKind,
    },
    Blocked {
        code: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub path: String,
    pub sha256: String,
    pub line: u32,
    pub confidence: Confidence,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Confidence {
    Direct,
    Inferred,
    Unknown,
}
impl DependencyRule {
    fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.reason.trim().is_empty() || self.evidence.is_empty() {
            return Err("rule requires ID, evidence and reason".into());
        }
        path(&self.locator.source_path)?;
        if !hex(&self.source_sha256, 64) || !hex(&self.expected_reference.guid, 32) {
            return Err("invalid rule hash or GUID".into());
        }
        if self.locator.field.is_empty() || self.locator.field.len() > 96 {
            return Err("invalid field depth".into());
        }
        for segment in &self.locator.field {
            if let FieldSegment::Key(key) = segment
                && key.trim().is_empty()
            {
                return Err("empty field key".into());
            }
        }
        for evidence in &self.evidence {
            path(&evidence.path)?;
            if !hex(&evidence.sha256, 64) || evidence.line == 0 {
                return Err("invalid evidence hash or line".into());
            }
        }
        owner(&self.locator.owner)?;
        match &self.disposition {
            DependencyDisposition::RenderSettingsTranslated { capability }
            | DependencyDisposition::ImportToolNotExecuted { capability } => {
                capability.validate()?;
            }
            DependencyDisposition::ProvidedBuiltin { provider_id, .. }
                if provider_id.trim().is_empty() =>
            {
                return Err("empty builtin provider ID".into());
            }
            DependencyDisposition::Blocked { code } if code.trim().is_empty() => {
                return Err("empty blocked code".into());
            }
            _ => {}
        }
        Ok(())
    }
}
fn owner(owner: &OwnerIdentity) -> Result<(), String> {
    match owner {
        OwnerIdentity::AssetMetadata {
            asset_guid,
            importer_kind,
        } => {
            if !hex(asset_guid, 32) || importer_kind.trim().is_empty() {
                return Err("invalid importer owner".into());
            }
        }
        OwnerIdentity::AssetDocument {
            asset_guid, kind, ..
        }
        | OwnerIdentity::ExpandedDocument {
            asset_guid, kind, ..
        } => {
            if !hex(asset_guid, 32) || kind.trim().is_empty() {
                return Err("invalid document owner".into());
            }
        }
        OwnerIdentity::SettingsDocument { kind, .. } if kind.trim().is_empty() => {
            return Err("empty settings kind".into());
        }
        OwnerIdentity::SettingsDocument { .. } => {}
    }
    if let OwnerIdentity::ExpandedDocument { instance, .. } = owner
        && !instance.split('@').all(|key| {
            key.split_once(':').is_some_and(|(guid, id)| {
                hex(guid, 32) && id.parse::<i64>().is_ok_and(|id| id != 0)
            })
        })
    {
        return Err("invalid expanded instance identity".into());
    }
    Ok(())
}
pub(super) fn hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn path(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.chars().any(char::is_control)
        || value.contains('\\')
        || value.contains(':')
        || value.starts_with('/')
        || value
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err("path must be canonical project-relative".into());
    }
    Ok(())
}
